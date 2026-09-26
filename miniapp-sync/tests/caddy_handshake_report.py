# -*- coding: utf-8 -*-
"""
解析 Caddy debug 日志，输出「每个 TLS 握手客户端的版本 / SNI / 结果」对照表。

用途：定位「某台设备连不上」时，确认它到底协商的哪个 TLS 版本、
      是被微信/路由拦了，还是 Caddy 已正常回包后被路径中间设备 RST。

前提：/etc/caddy/Caddyfile 头部已有 global { debug }。

用法：
  python tests/caddy_handshake_report.py                 # 默认 ssh aliyun，最近 60 分钟
  python tests/caddy_handshake_report.py aliyun 30
  python tests/caddy_handshake_report.py --file log.txt  # 直接解析已保存的日志

关键判读：
  772=TLS1.3, 771=TLS1.2, 770=TLS1.1, 769=TLS1.0（形如 39578 的是 GREASE 填充值）
  结果为「RST」= Caddy 已匹配证书并回包，包在路径上被中间设备重置（服务端无责）
"""
import argparse
import json
import subprocess
import sys

VERSION_NAMES = {772: '1.3', 771: '1.2', 770: '1.1', 769: '1.0'}


def fetch_log(ssh_host, minutes):
    cmd = (
        'sudo -n journalctl -u caddy --since \'%d min ago\' --no-pager 2>/dev/null'
        % minutes
    )
    result = subprocess.run(
        ['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=15', ssh_host, cmd],
        capture_output=True, text=True, timeout=120,
    )
    return result.stdout


def fmt_versions(raw):
    if not raw:
        return '-'
    named = [VERSION_NAMES.get(v, 'GREASE' if v > 771 else str(v)) for v in raw]
    # 去掉 GREASE 噪声，保留真实版本
    named = [n for n in named if n not in ('GREASE',)]
    return '/'.join(named) if named else '仅GREASE'


def parse(lines):
    """按出现顺序关联 client_hello -> matched certificate -> 握手结果。"""
    events = []
    pending = None  # 最近一次 client_hello 的 (versions, sni, ciphers)

    for line in lines:
        line = line.strip()
        # journalctl 每行带 syslog 前缀（"Sep 26 09:56:57 host caddy[123]: {...}"），去掉它
        brace = line.find('{')
        if brace < 0:
            continue
        line = line[brace:]
        try:
            entry = json.loads(line)
        except ValueError:
            continue

        msg = entry.get('msg', '')
        ts = entry.get('ts')

        if msg == 'event' and entry.get('name') == 'tls_get_certificate':
            hello = (entry.get('data') or {}).get('client_hello') or {}
            pending = {
                'versions': hello.get('SupportedVersions'),
                'sni': hello.get('ServerName') or '(空)',
                'ciphers': len(hello.get('CipherSuites') or []),
                'alpn': hello.get('SupportedProtos'),
                'ts': ts,
            }
            continue

        if msg == 'matched certificate in cache':
            ip = entry.get('remote_ip')
            port = entry.get('remote_port')
            if pending is not None:
                events.append(dict(pending, ip=ip, port=port, outcome='已选证书'))
                pending = None
            continue

        if msg.startswith('http: TLS handshake error from '):
            rest = msg[len('http: TLS handshake error from '):]
            peer, _, reason = rest.partition(': ')
            ip, _, port = peer.partition(':')
            reason = (reason or '').strip()
            if 'connection reset by peer' in reason:
                outcome = 'RST(被中间设备重置)'
            elif reason == 'EOF':
                outcome = 'EOF(客户端中途断开)'
            else:
                outcome = reason[:60] or '错误'
            if events and events[-1].get('outcome') == '已选证书' \
                    and events[-1].get('ip') == ip:
                events[-1]['outcome'] = outcome
            else:
                events.append({'ip': ip, 'port': port, 'outcome': outcome,
                               'versions': None, 'sni': '-', 'ts': ts})
            continue

    return events


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('ssh_host', nargs='?', default='aliyun')
    parser.add_argument('minutes', nargs='?', type=int, default=60)
    parser.add_argument('--file', help='直接解析本地日志文件而不是走 ssh')
    args = parser.parse_args()

    if args.file:
        with open(args.file, encoding='utf-8', errors='replace') as handle:
            lines = handle.readlines()
    else:
        text = fetch_log(args.ssh_host, args.minutes)
        if not text.strip():
            print('没有取到日志。确认 Caddyfile 已开启 global { debug } 且 ssh 主机名正确。')
            return 1
        lines = text.splitlines()

    events = parse(lines)
    if not events:
        print('日志里没有 TLS 握手事件。')
        return 1

    print('%-19s %-16s %-10s %-9s %s' % ('时间', '客户端IP', 'TLS版本', '结果', 'SNI'))
    print('-' * 88)
    for event in events:
        stamp = time_str(event.get('ts'))
        print('%-19s %-16s %-10s %-9s %s' % (
            stamp,
            '%s:%s' % (event.get('ip'), event.get('port')),
            fmt_versions(event.get('versions')),
            event.get('outcome'),
            event.get('sni'),
        ))

    print('\n===== 汇总：按 客户端IP + TLS版本 分组 =====')
    groups = {}
    for event in events:
        key = (event.get('ip'), fmt_versions(event.get('versions')))
        groups.setdefault(key, []).append(event.get('outcome'))
    for (ip, versions), outcomes in sorted(groups.items()):
        reset = sum(1 for o in outcomes if o.startswith('RST'))
        print('  %-16s 最高TLS %s ：共 %d 次握手，其中 RST %d 次' % (
            ip, versions.split('/')[0], len(outcomes), reset))
        if reset:
            print('      → 该客户端版本被路径中间设备重置（服务端已正常回包），换网络无效，需备案或改用 TLS 1.3')
        elif reset == 0 and outcomes:
            print('      → 该客户端版本握手正常')
    return 0


def time_str(ts):
    if ts is None:
        return '-'
    import datetime
    return datetime.datetime.fromtimestamp(
        float(ts), datetime.timezone(datetime.timedelta(hours=8))
    ).strftime('%m-%d %H:%M:%S')


if __name__ == '__main__':
    sys.exit(main())
