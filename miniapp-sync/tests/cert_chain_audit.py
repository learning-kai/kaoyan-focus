# -*- coding: utf-8 -*-
"""
证书链 / TLS 兼容性审计工具。

用途：排查「模拟器能连、真机 TLS handshake failed」这类问题。
它做两件事：

  1. 本地直连目标，转储服务器实际下发的证书链，并指出客户端信任库
     必须包含哪张根证书才能完成验证。
  2. 可选 --ssllabs：调用 SSL Labs 的公开 API，从第三方（AWS）视角确认
     服务器对外到底支持哪些 TLS 版本、证书链是否被判为不完整。

用法：
  python tests/cert_chain_audit.py                     # 只做本地链分析
  python tests/cert_chain_audit.py --ssllabs           # 追加第三方评估（耗时 1-3 分钟）
  python tests/cert_chain_audit.py --host example.com
"""
import argparse
import json
import os
import ssl
import socket
import sys
import tempfile
import time
import urllib.request


def decode_cert(der):
    """DER -> 人类可读的字段（借助 ssl 模块内部工具，无需第三方库）。"""
    tmpdir = tempfile.mkdtemp()
    path = os.path.join(tmpdir, 'c.pem')
    with open(path, 'wb') as handle:
        handle.write(ssl.DER_cert_to_PEM_cert(der).encode())
    return ssl._ssl._test_decode_cert(path)


def rdn_to_dict(rdn_tuple):
    out = {}
    for rdn in rdn_tuple:
        for key, value in rdn:
            out[key] = value
    return out


def dump_served_chain(host, port=443, timeout=15):
    print('=' * 72)
    print('[1] 服务器实际下发的证书链 (%s:%d)' % (host, port))
    ctx = ssl.create_default_context()
    with socket.create_connection((host, port), timeout=timeout) as sock:
        with ctx.wrap_socket(sock, server_hostname=host) as tls:
            chain = tls.get_unverified_chain()
            verified = tls.get_verified_chain()
            print('    协商协议: %s / %s' % (tls.version(), tls.cipher()[0]))
            print('    下发证书数: %d' % len(chain))

    names = []
    for index, der in enumerate(chain):
        info = decode_cert(der)
        subject = rdn_to_dict(info['subject'])
        issuer = rdn_to_dict(info['issuer'])
        names.append((subject.get('commonName'), issuer.get('commonName')))
        key_alg = info.get('publicKey', {}) if isinstance(info.get('publicKey'), dict) else {}
        print('    [%d] subject=%s' % (index, subject.get('commonName')))
        print('        issuer =%s' % issuer.get('commonName'))
        print('        有效期  %s -> %s' % (info.get('notBefore'), info.get('notAfter')))

    # 自签（subject == issuer）的那张就是客户端信任库里必须存在的锚点
    anchors = [name for name, issuer in names if name == issuer]
    print('    => 客户端信任库必须包含以下任一根证书才能完成验证:')
    for anchor in anchors:
        print('       * %s' % anchor)
    if not anchors:
        print('       * （服务器没下发自签根，取链尾 issuer 对应的根）')

    print('    本机验证结果: %s（验证路径 %d 张，锚点 %s）' % (
        '通过' if verified else '失败',
        len(verified or []),
        rdn_to_dict(decode_cert(verified[-1])['subject']).get('commonName') if verified else '-'))
    return anchors


def ssllabs_report(host, timeout_seconds=180):
    """第三方视角：SSL Labs 会报告协议支持、链是否完整、评级。"""
    print('=' * 72)
    print('[2] SSL Labs 第三方评估（从 AWS 发起，独立于本机网络）')

    base = 'https://api.ssllabs.com/api/v3/analyze?host=%s&all=done&publish=off' % host

    def fetch(use_proxy):
        handlers = [] if use_proxy else [urllib.request.ProxyHandler({})]
        opener = urllib.request.build_opener(*handlers)
        request = urllib.request.Request(base, headers={'User-Agent': 'cert-chain-audit'})
        return json.loads(opener.open(request, timeout=60).read().decode())

    data = None
    for use_proxy in (False, True):
        try:
            data = fetch(use_proxy)
            break
        except Exception as error:
            print('    请求失败 (proxy=%s): %s %s' % (use_proxy, type(error).__name__, error))
    if data is None:
        print('    SSL Labs 不可达，跳过第三方评估')
        return

    deadline = time.time() + timeout_seconds
    while data.get('status') not in ('READY', 'ERROR') and time.time() < deadline:
        print('    状态: %s，等待 10s ...' % data.get('status'))
        time.sleep(10)
        try:
            data = fetch(False)
        except Exception:
            try:
                data = fetch(True)
            except Exception as error:
                print('    轮询失败: %s' % error)
                return

    print('    最终状态: %s' % data.get('status'))
    for endpoint in data.get('endpoints') or []:
        if endpoint.get('statusMessage') != 'Ready':
            print('    端点 %s: %s' % (endpoint.get('ipAddress'), endpoint.get('statusMessage')))
            continue
        details = endpoint.get('details') or {}
        print('    端点 %s  评级 %s' % (endpoint.get('ipAddress'), endpoint.get('grade')))
        for protocol in details.get('protocols') or []:
            if isinstance(protocol, dict):
                print('      支持 %s %s' % (protocol.get('name'), protocol.get('version')))
        # SSL Labs 的链问题位掩码：1=不完整 2=包含锚点(多余) 4=含无关/重复证书
        for chain in details.get('certChains') or []:
            issues = chain.get('issues')
            text = []
            if issues & 1:
                text.append('不完整')
            if issues & 2:
                text.append('包含锚点证书')
            if issues & 4:
                text.append('含无关/重复证书')
            print('      证书链 issues=%s %s（长度 %d）' % (
                issues, '、'.join(text) if text else '正常', len(chain.get('certIds') or [])))
        cert = details.get('cert') or {}
        if cert:
            print('      叶子: %s | %s | %s %s 位' % (
                cert.get('subject'), cert.get('sigAlg'), cert.get('keyAlg'), cert.get('keySize')))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', default='api.skyhold.cloud')
    parser.add_argument('--port', type=int, default=443)
    parser.add_argument('--ssllabs', action='store_true', help='追加 SSL Labs 第三方评估')
    args = parser.parse_args()

    anchors = dump_served_chain(args.host, args.port)
    if args.ssllabs:
        ssllabs_report(args.host)

    print('=' * 72)
    print('排查提示：若本机验证通过但真机仍报 TLS handshake failed，')
    print('  1) 让手机换 4G/5G 再试 —— 能连上说明是当前 WiFi 的中间设备在拦 TLS；')
    print('  2) 用手机浏览器打开 https://%s/ 看是否能加载；' % args.host)
    print('  3) 老旧安卓（<7.1.1）信任库可能没有 %s。' % ' / '.join(anchors or ['ISRG Root X1']))


if __name__ == '__main__':
    main()
