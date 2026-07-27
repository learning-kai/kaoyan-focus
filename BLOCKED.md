npm run check:rust 的最终 Windows bin 测试因 os error 5（拒绝访问）失败；现有 smoke:core-flow 在 focus landing content 空白页超时。两者均为基线环境问题，未修改验收脚本或业务规避。
独立浏览器脚本已建立 Vite 启动与 root 探针，但尚未完成 CDP 真实交互断言；因此本目标暂不宣称完整验收通过。
