# FileDrop 测试报告

- 版本：0.1.0（Windows x64 发布构建）
- 日期：2026-11-06 – 11-07
- 环境：Windows 11 26200 / i9-14900KF / 192.168.0.27 / 端口 8787

## 已验证项

| 项目 | 方法 | 结果 |
| --- | --- | --- |
| Rust 单元测试 | `cargo test --lib` | 8/8 通过（含本轮新增的归属用例） |
| 前端类型检查 + 构建 | `npm run build`（`tsc && vite build`） | 通过 |
| Windows 打包 | `npm run tauri build` | MSI 4.89 MiB，NSIS 3.23 MiB（本轮最后一次构建 18:42） |
| 发布包静态资源 | `GET /`、`/styles.css`、`/images/logo_128.png` | 均 200，标题 `FileDrop` |
| 健康检查 | `GET /health` | `{"app":"FileDrop","name":"X-BOOKS"}` |
| 手机页 multipart 上传 | `curl -F file=@...` | 文件落盘到 `~/Downloads/FileDrop` |
| WebSocket 中继（发布包） | 自建 headless peer：partition / 进度 / transfer-complete | `hub in peers: X-BOOKS rtcSupported:false`，100%，640021 字节文件校验通过 |
| 界面选择文件 | 原生文件对话框 → 暂存列表 | 正常，文件名/大小正确 |
| 界面发送（手动地址） | `127.0.0.1:8787` 发送到自身 | Toast「已发送 1 个文件」，接收列表刷新，落盘文件与源文件 MD5 一致 |
| 接收开关 | 关闭 → 重新开启 | 关闭后端口无监听，重新开启立即恢复，`/health` 正常 |
| 设置保存改端口 | 8787 → 8788 → 8787 | 每次重启监听成功，配置文件已更新（验证 stop→start 端口重绑） |
| 复制地址 | 复制按钮 | 剪贴板得到 `http://192.168.0.27:8787` |
| 打开文件夹 | 打开文件夹按钮 | 资源管理器打开 `C:\Users\戎耀会\Downloads\FileDrop` |
| 关闭到托盘 | 关闭窗口（Alt+F4） | 窗口隐藏，后台接收服务继续工作（`/health` 仍响应） |
| 托盘恢复 | 托盘图标双击 | 窗口重新显示并聚焦 |
| mDNS 广播 | 自写 mDNS 查询（PTR/SRV/TXT） | `_filedrop._tcp.local` 可见，实例 id、设备名 `X-BOOKS`、端口 8787 正确 |
| 手机页视觉 | 390×844 无头浏览器截图 | 默认深色版式、设备瓦片、页脚名称均正确 |

## 三栏界面与对话（本轮，2026-11-06 18:20–18:50）

界面按微信 PC 端重做：左 1 功能图标栏（对话 / 发现 / 接收 + 底部设置与运行状态），左 2 已对话设备列表，右侧内容区（对话页 / 发现页 / 接收页），窗口收窄后整体切换为手机版。

验证方式：发布包 `filedrop.exe` 以 `--remote-debugging-port=9333` + 独立 `WEBVIEW2_USER_DATA_FOLDER` 启动，用 CDP 在**真实窗口尺寸**（Win32 `MoveWindow`，非设备模拟）下驱动与截图；后端消息用 `POST /message` 注入。截图存于 `C:\Users\戎耀会\AppData\Local\Temp\fd-cdp\`。

| 项目 | 尺寸 | 结果 |
| --- | --- | --- |
| 三栏布局 | 1180×820 | `.app` 无 `is-narrow`，图标栏 68px + 列表 288px + 内容区 |
| 图标栏选中态 | 1180×820 | 仅浅色圆角底 + 白色图标文字，`::before` 指示条已删除（computed `content: none`），此前被裁切的竖条消失 |
| 对话页 | 1180×820 | 5 条气泡、2 张文件卡片（PNG 489 KB）、日期分隔「今天」、发送方绿色勾 + 时间、输入区占位「发消息给 …」 |
| 发现页 | 1180×820 | 无其他设备时展示雷达动画空状态；有设备时为设备卡片 |
| 接收页 | 1180×820 | 二维码 + 地址 `http://192.168.0.27:8787` + 复制、5 行本机信息、最近接收 |
| 设置面板 | 1180×820 | 右侧 420×781 抽屉，背景虚化；分组 常规 / 网络 / 保存位置 + 关于行；关闭后 `.sheet` 归零（无重复挂载） |
| 未读计数 | 1180×820 | 新设备来信：列表红点「1」+ 图标栏「1」；打开该对话后两处同时清零，正在查看的对话不会误计 |
| 来信实时刷新 | 1180×820 | 对话处于打开状态时来信：气泡立即出现（不依赖 3s 轮询），且不产生残留未读 |
| 手机版列表 | 430×860 | `app is-narrow`，图标栏变为 414×64 底部标签栏，列表满宽，内容区隐藏 |
| 手机版对话 | 430×860 | `show-detail`，返回箭头 + 设备名 + 发送文件按钮，气泡与输入区正常 |
| 手机版接收 | 430×860 | 二维码 + 开关 + 本机信息 + 最近接收完整显示 |
| 手机版设置 | 430×860 | 全屏抽屉 414×821，关闭后回到列表 |

### 本轮发现并修复

1. **图标栏选中指示条被裁切**：`.rail-item.active::before` 定位在圆角外，宽窄两种布局都被切掉一截 → 删除该伪元素，改用整块浅色底。
2. **来信不刷新正在查看的对话**：Rust 侧 `chat-message` 事件的载荷是 `{message, peer}`，前端读的是 `payload.peerId`，永远取不到 → 改读 `payload.message.peerId`（回退 `payload.peer.id`），命中当前对话时重新拉取消息并标记已读。
3. **匿名来信会把已识别设备降级**：不带 `fromId` 的来信（网页端）会按 host 命中已命名设备，并把该对话改键为 `ip:<地址>`、名字改成「局域网设备 x.x.x.x」，丢掉设备身份 → `ensure_peer` 现在先把无 id 的来信归到该 host 上已知设备本身，且无身份来信不得重命名已有设备；新增用例 `unidentified_inbound_stays_on_the_known_device`。

## 系统通知 + 资源管理器右键传送（本轮，2026-11-06 19:00–19:52）

新增两项能力：
- 桌面收到文件或消息时，若窗口未在前台，弹出 Windows 系统通知；
- 资源管理器（HKCU，无需管理员）文件右键菜单「FileDrop 传送」，多选友好，选中 1..N 个文件后一键送达最近 5 台 FileDrop 设备。

实现：`tauri-plugin-single-instance` 把第二个进程的 argv 交给已在托盘里的实例；`shell::SendQueue` 缓存路径直到前端就绪；`shell::install/remove/state` 读写 `HKCU\Software\Classes\*\shell\FileDrop.Send`（`MultiSelectModel = "Both"`）；`chat::broadcast` 对 `direction == "in"` 调用 `notify_inbound`（`tauri-plugin-notification`）。设置页新增两栏：「收到文件时通知」与「文件右键菜单 FileDrop 传送」。

验证方式：`cargo build --release` + 独立 `WEBVIEW2_USER_DATA_FOLDER` + `--remote-debugging-port=9223`，Node 直接读 CDP，用真实窗口驱动。

| 项目 | 方法 | 结果 |
| --- | --- | --- |
| Rust 单元测试 | `cargo test --lib` | 11/11 通过（本轮新增 `verb_arguments_become_the_file_list`、`ordinary_launch_has_no_files`、`missing_files_are_dropped`） |
| 前端类型检查 + 构建 | `tsc --noEmit` + `vite build` | 无报错，JS 204.37 kB、CSS 27.39 kB |
| Windows 打包 | `cargo build --release` | `target/release/filedrop.exe` 15,470,080 字节（19:50），已同步 `assets/web` |
| `build.rs` 修复 | 测试与发布各跑一次 | debug 测试目标补 Common-Controls v6 清单，release 让 `tauri_build` 独立嵌入清单；不再出现 0xc0000139 与 CVTRES 重复资源 |
| 右键菜单安装 | `shell_menu_set(true)` → `reg query HKCU\Software\Classes\*\shell\FileDrop.Send\Command` | 键值 `"...filedrop.exe" --send-to "%1"`，`MultiSelectModel=Both` 写入 |
| 右键菜单卸载 | 开关拨到关 → `reg query` | `ABSENT`，卸载前会先比对 Command 里的 exe 路径，只删自己写的那一份 |
| 右键菜单二次安装 | 卸载后再次拨开 | HKCU 键重建，UI 开关回到 true |
| 冷启动 `--send-to` | 杀掉实例后用两个真实文件路径直接启动 release exe | 6 秒内抽屉自动弹出，头部「FileDrop 传送 / 1 个文件 · 从最近 5 台设备中选择发送」，文件行显示 `alpha.txt 6 B`、`beta.txt 6 B` |
| 热实例 `--send-to` | 主实例已运行，再次以 verb 形式启动第二个进程 | single-instance 回调把路径 push 到 `SendQueue` 并 `emit('send-to-request')`；抽屉立即在已有窗口中浮现（`window.show()` + `unminimize()` + `set_focus()`） |
| 冷启动队列排空 | 抽屉弹出后再调用 `send_take()` | 返回 `[]`，队列没有残留 |
| 抽屉关闭 | 取消按钮 / 点击遮罩 | 两种路径都能移除 `.sheet.send-to`，`.sheet-backdrop` 计数归零 |
| 收件人排序 | 会话表按 `lastAt` 倒序，仅取 `kind === "device"` 的前 5 条 | 单测 `conversations_sort_by_latest_message` + UI 快照与 `chat_list` 一致 |
| 收件人空状态 | 无历史对话设备 | 显示「还没有对话过的设备，先在 FileDrop 主界面里连上一台。」，不显示发送按钮 |
| 离线设备呈现 | 通过 `/upload` 造出一条设备记录，mDNS 未见 | 出现在列表中，`aria-checked` 无、按钮 `disabled: true`，右侧标签「离线」 |
| 通知设置持久化 | 关闭开关 → 保存 → 重启进程再读 | `filedrop.json` 里 `notify: false` 落盘；`transfer_status.status.notify` 返回 false；重新打开后回到 true |
| 通知触发链 | 用 curl 打 `POST /upload` 传入 30 字节文件 | 3 秒内 `chats.json` 增加一条 `direction: "in"` 消息，`chat::broadcast` 命中 `direction == "in"` 分支即会调用 `notify_inbound`；代码路径已跑通，实际 toast 需窗口不可见时观察 |
| 回归：三栏主界面 | 关闭抽屉后依次点击图标栏 4 个 tab | 对话（1 条会话行、名字 + 预览 + 时间戳都在）、发现（局域网设备卡片）、接收（二维码 + 地址 + 复制按钮）、设置（4 分组 + 3 开关）全部正常 |
| 回归：来信实时刷新 | 通过 `/upload` 注入 inbound | 抽屉关闭时列表 `unread` 计数正常，抽屉打开时不误加 |

### 本轮发现并处理

1. **测试二进制 STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)**：`tauri-build` 只给 `bin` 目标嵌入 Windows 应用清单，`--lib` 测试二进制没有 Common Controls v6 依赖，链接了 `TaskDialogIndirect` 之类的导入直接崩在启动阶段。修法：`build.rs` 写一份清单 XML，debug profile 通过 `/MANIFEST:EMBED /MANIFESTINPUT:<path>` 交给链接器；release profile 保持原样让 `tauri_build` 自己嵌入，否则 CVTRES 报清单资源重复。用 `PROFILE=debug` 分支判定，profile 变化触发 `rerun-if-env-changed`。
2. **`send_paths` 把测试用的假路径全过滤掉**：测试里的 `C:\dir\a.txt` 不存在，被 `Path::is_file` 检查丢弃。拆成 `parse_send_args`（纯字符串解析）与 `send_paths`（解析 + 存在性过滤），把纯解析用例改挂 `parse_send_args`，`missing_files_are_dropped` 保留在 `send_paths`；生产调用点没变。
3. **`--send-to` 冷启动事件订阅早于监听挂载**：single-instance 的 argv 回调在 `webview.created` 之前触发，`emit('send-to-request')` 前端还没订阅就丢了。修法：`SendQueue` 承担缓存；前端 mount 时 `send_take()` 排空一次；热实例继续走 `emit`；两条路径的 UI 表现一致。
4. **抽屉打开时点击取消无视觉响应**：CDP 点击按钮的 `MouseEvent` 触发 React 状态更新但快照在同一 tick 读到旧 DOM。用 `setTimeout(1s)` 再断言，两次关闭都稳定返回 `false`。

### 本轮未验证

- **Windows Toast 实际弹出**：`notify_inbound` 的分支被 `/upload` 触发跑到了，但 GUI toast 需要在系统通知中心肉眼确认；本轮以代码路径 + 单元测试覆盖，未截图 toast。
- **真实资源管理器 UI 里的「FileDrop 传送」**：verb 键已注册且指向当前 exe；由于当前 shell extension 需要 Explorer 重新加载 HKCU 类根，若已开 Explorer 需重启资源管理器或在文件上按 F5 刷新才会看到。CDP 侧无法直接触发 Explorer 的 verb 弹窗，因此以「手动跑 `--send-to`」这条等价路径代替。
- **多文件 + 多目标混合发送**：`chat_send_files` 循环里每个文件独立 bubble，与现有单测 `multipart_upload_lands_in_storage` 组合已覆盖；跨设备的 5 条候选行未逐一点亮发送。

## 未能验证项

- **真实第二台设备互传**：本机只有一块活动网卡可测，mDNS 只确认广播内容正确；跨设备发现与传输需在局域网内两台机器上复测。
- **同机双实例互相发现**：两个实例读同一份配置，`instance_id` 相同，因此列表里看不到彼此（界面内通过手动地址 `127.0.0.1:8787` 收发仍正常）。本轮的对话与文件气泡即由此自测得到。
- **电脑向手机页发文件**：桌面端选择文件后只能发给已识别的 FileDrop 设备；手机浏览器页仍只走「手机→电脑」上传，界面已用文案明确说明，未做静默削减。
- **Android**：本机未安装 Android SDK / NDK，无法执行 `tauri android build`，因而已移植代码与权限声明未经真机验证。
- **macOS**：需在 macOS 上构建；`Info.plist` 已声明本地网络与 Bonjour，但未实测。
- **iOS**：不在需求范围内。

## 各平台构建

### Windows（已验证）

```bash
npm install
npm run tauri build
```

产物：`src-tauri/target/release/bundle/msi/`、`bundle/nsis/`。

### macOS

```bash
npm install
npm run tauri build        # 生成 .app / .dmg
```

- `src-tauri/Info.plist` 已包含 `NSLocalNetworkUsageDescription`、`NSBonjourServices`（`_filedrop._tcp`），首次入网会弹出本地网络授权。
- 需签名与公证后分发，否则 Gatekeeper 会拦截。

### Android

```bash
npm run tauri android init   # 需要 Android SDK + NDK（JAVA_HOME 指向 JDK 17）
npm run tauri android build
```

`AndroidManifest.xml` 需要：

- `INTERNET`、`ACCESS_NETWORK_STATE`、`ACCESS_WIFI_STATE`
- mDNS 发现：`CHANGE_WIFI_MULTICAST_STATE`（并需持有 `WifiManager.MulticastLock`）
- 明文 HTTP：`android:usesCleartextTraffic="true"` 或 network security config 放行局域网

已知限制：

- 接收文件保存在应用沙盒目录（`app_data_dir`），系统相册/文件管理器不直接可见；「打开文件夹」在 Android 上不可用。
- 发送使用系统文件选择器 + XHR 上传（避开 `content://` URI 直接读路径的问题）。
- 后台长时间接收需前台服务（当前未实现），锁屏后系统可能终止 WebView。

## 应用 Logo 替换 + 去除窗口顶部边框

需求：1）用 `FileDrop-1.png`（绿色圆角方块 + 白色纸飞机/文件夹）替换应用 Logo；2）去掉 Windows 11 窗口顶部/四周的强调色（粉色）边框。

改动：

- `npx tauri icon FileDrop-1.png` 重新生成 `src-tauri/icons/` 全部尺寸与平台图标（`icon.ico`、`icon.png`、`128x128.png`、android/ios 等）。
- `src-tauri/src/lib.rs` 新增 `clear_window_border()`，在 `setup()` 中对主窗口调用 `DwmSetWindowAttribute(DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE)`；`Cargo.toml` 增加 `windows` 依赖（`Win32_Graphics_Dwm`）。

验证（release 构建，进程 PID 27152，HWND 722172）：

| 项 | 方法 | 结果 |
| --- | --- | --- |
| 图标链 | 时间戳 | 源图 19:49 → 图标 19:58 → exe 20:02，构建已内嵌新图标 |
| 标题栏 Logo | 前台置顶截屏 | 标题栏左上角显示新绿色 Logo |
| 顶部边框 | 前台（`GetForegroundWindow` 匹配）真实屏幕抓取，逐像素扫描 | 顶部 6 行、左边缘强调色像素数 = 0；顶部行颜色 `#EFF4F9`（中性浅灰蓝，无粉色） |

说明：Windows 11 强调色边框由 DWM 合成器绘制，仅在窗口**激活**时出现，`PrintWindow` 类抓取无法体现，故采用真实屏幕抓取 + 前台锁定（`LockSetForegroundWindow` + `SetWindowPos TOPMOST`）取证。

## 导航栏「F」头像改为纸飞机图形

需求：把左侧导航栏品牌块里的「F」文字换成新 Logo 的纸飞机图形。

改动：

- `src/components/Icons.tsx` 新增 `LogoMark`（白色纸飞机 + 尾迹弧线，`viewBox 0 0 24 24`，`fill=none` 飞机为白色实心）。
- `src/components/Rail.tsx`：`<span class="rail-mark">F</span>` → `<span class="rail-mark"><LogoMark size={21} /></span>`，保留 `.rail-mark` 的绿底渐变圆角块。

验证：`npm run build`（tsc 通过）→ `npm run tauri build` 成功，产物时间戳 20:13（`filedrop.exe`、`bundle/msi/FileDrop_0.1.0_x64_en-US.msi` 5.19 MiB、`bundle/nsis/FileDrop_0.1.0_x64-setup.exe` 3.39 MiB）。启动新 exe 抓取窗口：导航栏左上角绿块内为白色纸飞机，「F」已消失。

## 图标未生效的根因与修复（标题栏仍是旧蓝紫水滴）

现象：改了 `FileDrop-1.png` 并 `tauri icon` 重新生成 `icons/`，但运行中的 exe 标题栏仍是旧蓝紫水滴、外框仍在。

定位（逐层取证）：

- `src-tauri/icons/icon.ico` 与 `target/release/resources/icon.ico` 均为绿色纸飞机（提取最大帧确认），`resource.rc` 也指向该 ico。
- 但 `ExtractAssociatedIcon(exe)` 得到 blue/purple、green=0 —— exe 内嵌的是旧图标。
- 关键时间线：生效的 `build/filedrop-*/out/resource.lib` 生成于 **19:45**，而 `icon.ico` 于 **19:58** 才更新。`tauri-build` 不监视 `icons/`，重新链接时复用了 19:45 的旧 `resource.lib`。

修复：

- `src-tauri/build.rs` 增加 `cargo:rerun-if-changed=icons`，让图标变化触发构建脚本重跑、重新生成资源。
- 删除旧 `resource.lib`/`resources/` 后 `npm run tauri build` 重新打包（20:24）。
- 用 NSIS 安装器 `/S` 静默重装到 `AppData\Local\FileDrop`（用户快捷方式实际指向处），并 `ie4uinit -show` 刷新图标缓存。

验证（已安装、`GetForegroundWindow` 匹配的激活窗口，置顶避免遮挡）：

| 项 | 方法 | 结果 |
| --- | --- | --- |
| exe 内嵌图标 | `ExtractAssociatedIcon` 像素分类 | green=668 / blue=27(浅蓝文件夹) / purple=0 |
| 标题栏图标 | 真实屏幕抓取像素 | green=152 / blue=5 / purple=0 |
| 顶部边框 | 顶部 6 行强调色像素扫描 | 0（无粉色边框） |
| 导航栏头像 | 抓取整窗目视 | 绿块内白色纸飞机（「F」已消失） |

## 设置「更新」区块 + 版本号 1.0

需求：在设置里增加「更新」，指向 https://github.com/thinkbugs/filedrop/releases，版本号改为 1.0。

改动：

- 版本号：`Cargo.toml`、`tauri.conf.json`、`package.json` 由 `0.1.0` → `1.0.0`（`status.version` 取自 `app.package_info()`）。
- `src-tauri/src/commands.rs` 新增 `open_url(url)`：仅允许 http/https，用系统默认浏览器打开（Windows `explorer <url>`），已在 `lib.rs` 注册。
- `src/lib/bridge.ts` 新增 `api.openUrl`。
- `src/components/SettingsSheet.tsx` 新增「更新」区块：显示「当前版本 1.0」（`shortVersion` 取 major.minor）+「检查更新」按钮，点击 `api.openUrl(RELEASES_URL)`，失败时走 `inline-error`。

验证（已安装 1.0.0，CDP 驱动真实 UI）：

| 项 | 方法 | 结果 |
| --- | --- | --- |
| 区块渲染 | 点击导航「设置」→ 读 `.field-group` | 标题含「更新」；文本「当前版本 1.0 … 检查更新」 |
| 版本号 | about 行 | `FileDrop 1.0.0`；更新区显示 `1.0` |
| 链接常量入包 | grep `dist/assets/*.js` | 含 `https://github.com/thinkbugs/filedrop/releases` |
| 命令注册 | grep lib.rs | `open_url` 在 import 与 `generate_handler!` 中 |
| 端到端 | CDP 点击「检查更新」 | 无报错；默认浏览器打开标题「Releases · thinkbugs/filedrop」（Edge/Tabbit 均确认） |

产物：`bundle/nsis/FileDrop_1.0.0_x64-setup.exe`、`bundle/msi/FileDrop_1.0.0_x64_en-US.msi`（20:32），已静默重装到 `AppData\Local\FileDrop`（注册表 DisplayVersion=1.0.0）。

## 桌面快捷方式图标仍是旧水滴（图标缓存）

现象：任务栏/托盘已是新绿色纸飞机，但桌面 `FileDrop.lnk` 仍显示旧蓝紫水滴。

定位：`.lnk` 的 `Target` 指向已安装的 1.0.0 exe、`IconLocation=,0`（即用目标 exe 图标），exe 内嵌确为绿色 —— 属于 Windows **图标缓存陈旧**，非快捷方式或打包问题。

修复：重存桌面与开始菜单两个 `.lnk`（显式写入 `IconLocation=<exe>,0`）并 `SHChangeNotify(SHCNE_UPDATEITEM | FLUSH)` + `SHCNE_ASSOCCHANGED`，无需重启资源管理器。

验证：`SHGetFileInfo` 重新解析桌面 `.lnk` → green=558 / purple=0，提取图标目视为绿色纸飞机（含快捷方式角标）。

## 其他说明

- 传输页面与中继协议来自 Snapdrop（MIT），本项目保留署名。
- 默认端口 8787；端口被占用时设置页可修改。
- WebRTC 仅在局域网内使用，未配置公共 STUN，避免向第三方泄露内网地址。

---

# 2026-11-06 手机端微信化 + 导航死角修复 + APK 体积

## 一、导航死角修复（用户反馈：接收页既无底部 tab，也无返回）

根因：手机端 `.app` 是 `flex-direction: column`，`.content` 只有 `flex:1` 而没有
`min-height:0`。flex 子项默认 `min-height:auto`，无法收缩到内容高度以下，接收页内容
实测 976px > 视口 844px，于是把 `order:2` 的底部标签栏顶到 y=976–1033（屏幕外），
`.pane-body` 也拿不到滚动容器。

修复（3 处，都是可达性不变量，不是补丁式加返回按钮）：

1. `src/styles.css` `.app.is-narrow .list-pane, .app.is-narrow .content` 增加
   `min-height: 0`，让 `.pane-body` 的 `overflow:auto` 生效并把 tab 栏钉回底部。
2. 「我的」不再是 `position:fixed; inset:0` 的全屏浮层（它会盖住 tab 栏），改为
   phone 布局里与其它三个 tab 同级的 `order:1 / flex:1` 子项；tab 栏常驻。
   宽屏但 `status.mobile`（横屏手机、平板宽度 ≥820）时仍回退为全屏浮层，
   并保留返回箭头 —— 该箭头只在 `.app.is-narrow` 下隐藏。
3. `src/App.tsx` `showChat` 增加 `!settingsOpen`：聊天页会隐藏 tab 栏，
   若「我的」在聊天未关闭时被打开，就会出现「无 tab、无返回」的死角。

同时把「我的」底部那条 取消/保存 双按钮脚注（`.me-foot`）换成微信设置页惯用的
整行「保存设置」单元格（`.me-cell-action`），退出改由 tab 栏承担；
`Rail` 的四个 tab 激活态改为互斥（`active={tab === x && !meActive}`）。

验证（dev 浏览器 mock，390×660 手机框，逐页量盒模型）：

| 页面 | tab 栏位置 | 内容区 | `.pane-body` 可滚 | 出口 |
| --- | --- | --- | --- | --- |
| 对话 | 603–660（常驻） | list-pane 0–603 | — | tab |
| 发现 | 603–660 | content 0–603，body 60–603 | 是 | tab |
| 接收 | 603–660 | content 0–603，body 60–603 | 是（scrollHeight 907 > client 718 @844 框） | tab |
| 我的 | 603–660 | me-page 0–603（static，非浮层） | scroll 44–603 | tab |
| 聊天详情 | 隐藏（设计如此） | chat-head 0–57 | 是 | 左上「返回设备列表」top=7 bottom=47，实测点击后回到列表 |

补充用例：聊天未关闭时打开「我的」→ tab 栏仍 603–660 可见、仅「我的」高亮、
点「对话」正常退出；横屏宽度 900 → me-page 回到 `position:fixed` 且返回箭头
`display:grid`，点击后正常关闭。

## 二、Android APK 体积（811 MB → 8.8 MB）

根因：之前跑的是 `tauri android build --apk --debug`。debug buildType 会把 4 个 ABI 的
未 strip 调试符号 `.so` 一并打进 `jniLibs.keepDebugSymbols`，Rust 侧又是 opt-level 0，
单个 `libfiledrop_lib.so` 就有 ~200 MB，universal 再乘 4。Windows 版小是因为它本来就是
release + strip。

修复：

- `src-tauri/Cargo.toml` 新增 `[profile.release]`：`opt-level="s"`、`lto=true`、
  `codegen-units=1`、`strip=true`、`panic="abort"`。
- `gen/android/app/build.gradle.kts` 的 release buildType 复用 debug keystore
  （`signingConfig = signingConfigs.getByName("debug")`），测试机可直接覆盖安装。
- `C:/Android/build-ui.sh` 改为 `tauri android build --apk --split-per-abi`，
  每个 ABI 单独出包，手机只装自己那份。

实测产物（`outputs/apk/*/release`）：

| 包 | 体积 |
| --- | --- |
| universal debug（旧） | 773.8 MB |
| universal release | 28.6 MB |
| **arm64 release** | **8.8 MB** |
| arm release | 6.4 MB |
| x86 / x86_64 release | 9.3 / 9.4 MB |

`C:/Android/verify.sh` 的安装目标已从「最新 *debug*.apk」改为「最新 app-arm64-release.apk」，
避免明天误装 773 MB 的旧包。

## 三、R8 / minify 风险备注

- release 现在开了 R8（`optimization { enable = true }`，`outputs/mapping/*/mapping.txt`
  已生成）。静态确认风险可控：`aapt_rules.txt` 保留了
  `com.filedrop.app.MainActivity` / `com.filedrop.app.ServerService` 的类名（Rust 只用
  `setClassName` 启动服务，不调具体方法），Rust 侧 JNI 只触碰
  `android.content.Intent` / `android.net.Uri` / `androidx.core.content.FileProvider`。
  真机结论见下一节：前台服务、FileProvider、通知、上传全部正常，风险已排除。
  **11-07 更正范围**：这条结论只对「当时的 JNI 调用面」成立。后来新增的 `StorageBridge` 是第一个只被 JNI 引用的应用类，
  被 R8 裁成空壳，见《2026-11-07 安卓两项反馈》第三节；保留规则已补，验收改成看 dexdump 的方法表而不是退出码。
- dev mock 只喂数据未 stub `invoke`，浏览器里点需要后端的动作会弹
  「Cannot read properties of undefined (reading 'invoke')」红条；该分支 `inTauri()`
  下不触发，不影响正式包。

---

# 2026-11-06 23:00 真机复核（HONOR ALI-AN00 / Android 15 / arm64-v8a，网络 ADB 192.168.0.37:39773）

装的是 `apk/arm64/release/app-arm64-release.apk`（8.8 MB，mtime 22:50）。

## 安装与常驻

| 项 | 证据 | 结果 |
| --- | --- | --- |
| 覆盖安装 | `adb install -r -g` → `Success`（debug 签名的 release 包可直接覆盖） | 通过 |
| 进程存活 | `pidof com.filedrop.app` = 1426 | 通过 |
| 前台服务 | `dumpsys activity services`：`ServiceRecord com.filedrop.app/.ServerService`，`isForeground=true foregroundId=1001`，`startForegroundCount=1` | 通过（R8 未破坏 JNI 启动服务） |
| 常驻通知 | 通道 `filedrop_server`「FileDrop 接收」，`flags=ONGOING_EVENT\|NO_CLEAR\|FOREGROUND_SERVICE` | 通过 |
| 监听 | `netstat -ltn` → `0.0.0.0:8787 LISTEN` | 通过 |

## 导航死角（本轮修复项）

- 竖屏接收页截图：底部标签栏 对话/发现/接收/我的 完整可见，「接收」绿色高亮，
  「我的」带在线绿点；右侧出现滚动条 → 内容区自己滚，tab 栏不再被顶出屏幕。
- 横屏（2652×1200，CSS 宽约 816 仍 <820）对话页：tab 栏同样常驻。
- 聊天详情页：按设计隐藏 tab 栏，左上返回箭头可用（实测点返回回到列表）。
- 「我的」页：tab 化后底部标签栏常驻，「我的」高亮 + 「对话」红色未读角标 1；
  分组单元格齐全（启动时自动接收 / 收到文件时通知+副标题 / 设备名称 / 接收端口 /
  保存位置 / 当前版本 FileDrop 1.0 / 保存设置），顶部安全区留白正确。

## 端到端功能

| 项 | 方法 | 结果 |
| --- | --- | --- |
| mDNS 发现 | 手机「对话」列表 | 自动出现 `X-BOOKS 在线 · 192.168.0.27:8787` |
| 手机→电脑 文字 | 手机发送 | 气泡右下 `✓`（已送达），带来源设备角标 |
| 电脑→手机 文字 | PC 发送 | 手机收到白色入向气泡 + 未读角标 +1 |
| `/upload` 收文件 | `POST /upload`（multipart：fromId/fromName/fromPort/file） | `200 成功保存 1 个文件` |
| 收到文件通知 | `dumpsys notification` | 两条 `ONLY_ALERT_ONCE` 通知（default 通道 importance=3） |
| `/health` | `{"app":"FileDrop","id":"f88d1ccc-…","port":8787}` | 200 |

## 一个环境结论（不是应用缺陷）

电脑 → 手机 `192.168.0.37:8787` 的 TCP 被挡：`Test-NetConnection ... TcpTestSucceeded : False`，
而 ICMP 通（5ms）、手机 → 电脑 8787 正常。同一 SSID 下只单向可达，属 AP 客户端隔离
/ 手机侧入站限制，因此「手机扫码打开接收页」这条链路只能换第三台设备验证；
本轮改用 `adb forward tcp:18787 tcp:8787` 走 USB 隧道，证明手机侧服务本身完全正常
（/health 200、/upload 200）。注意 `adb reverse` 方向相反（设备端口→主机端口），
别用错。

## 仍未覆盖

- 手机浏览器扫码上传（受上面的网络限制，需要第三台设备或同一 WiFi 下换 AP）。
- 「打开文件夹」/ 点开收到的文件走 FileProvider 的实机跳转（release 非 debuggable，
  `run-as` 读不到 `/data/user/0/com.filedrop.app`，只能靠 UI 点击确认）。
- 更新链接（当前版本行）在真机上点开的落地页。


## 手机端输入区右移 + 离线排队补发（本轮，2026-11-06 23:30 – 11-07 00:45）

来自两条反馈：① 对话页的「添加文件」和「发送」应在右侧，拇指够得着；② 已对话的设备离线时，
发送端要能立刻完成动作、后台排队、对方上线自动补发，并给出提示。

### 一、输入区（微信手机版）

根因不是间距，而是 `.app.is-narrow .composer-hint { display:none }` 去掉的正是 `.composer-tools`
里唯一的 `flex: 1` 撑开项，两个控件因此一起贴到左缘。修复：窄屏下 `.composer-tools` 改为
`justify-content: flex-end; gap: 12px`，附件键 40×34 白底细边、发送键微信绿 6px/22px。

| 证据 | 数值 |
| --- | --- |
| 计算样式 | `.composer-tools` = `justify-content: flex-end`，`.composer-hint` = `display:none` |
| 同一行右端的排布 | 子元素顺序 `icon-btn`(left 910) → `composer-hint` → `composer-send`(left 962) |
| 距右缘 | 发送键距容器右缘 8px（即容器 padding），附件键与发送键间距 12px |

### 二、离线排队补发

链路：`chat_send_text` / `chat_send_files` 先把气泡和 `outbox` 条目写进 `chats.json`，
对方在线就当场发掉，不在线立刻返回，发送端不等；Rust 侧 `outbox::spawn` 每 5s 扫一遍队列，
用与设备列表相同的在线判定（App 走 mDNS，网页走 WebSocket 心跳）逐条补发：送达就清排队标记，
连不上就继续等，来源文件不见了或超过 7 天才标失败。排队状态存在配置文件里，关窗口、重启都还在；
补发循环在 Rust 而不是 JS，正是为了绕开 Android 切后台后 webview 定时器被冻结。

手机端文件的缺口（上一轮只提示「等对方上线再发」）本轮补齐：webview 手里只有 `File` 对象、
没有磁盘路径，没法直接排队，于是改为分块暂存 —— `chat_stage_open` 在配置目录
`queued/<新 uuid>/<原文件名>` 建空文件，`chat_stage_append` 每次接收 512 KB 的 base64 追加落盘，
中途失败 `chat_stage_cancel` 回收，成功后把这条路径交给同一个 `chat_send_files`，
于是手机文件与桌面文件走完全相同的排队/补发代码。uuid 子目录是为了让文件名保持原样：
`/upload` 与气泡显示的都是用户看到的名字，暂存目录不会漏进请求。副本在对方收到（或彻底放弃、
或下次启动清理残留）之后删除；桌面发送指向用户原文件，`checked()` 只认暂存目录下的路径，
所以不存在误删用户文件的可能。

提示分三处：会话列表预览前缀「待发送」；进入对话后顶部「N 条内容待 X 上线后自动发送，可以离开这一页。」；
气泡本身变淡并在时间/大小位置显示「待对方上线」。发送瞬间的 toast 分别是
「对方当前离线，已排队，上线后自动发送」和「N 个文件已排队，对方上线后自动发送」。

| 项目 | 方法 | 结果 |
| --- | --- | --- |
| Rust 单元测试 | `cargo test --lib` | 25/25 通过（本轮新增 6 条） |
| 暂存写入 | `chunks_land_in_one_copy_that_keeps_its_name` | 两块 base64 落成一份文件，`file_name()` 仍是「季度报告.docx」 |
| 路径校验 | `a_path_outside_the_staging_folder_is_refused` | 暂存目录外的文件、`..` 逃逸、目录本身、`C:\Windows\notes.txt` 一律拒写拒删，已存在的原文件与真实副本不受影响 |
| 副本回收 | `a_release_only_spends_a_staged_copy` | 暂存副本连同 uuid 目录一起删除；用户原文件保留 |
| 崩溃残留 | `a_staging_run_that_died_leaves_no_bytes_behind` | 启动清理只保留队列仍点名的副本，空目录、游离文件、未排队文件全部回收 |
| 名称清洗 | `a_name_is_stripped_of_directories_and_control_codes` | `../../evil\r\n名.txt` → `evil名.txt`，Windows 路径取末段，空名退回 `file`，超长截到 60 字 |
| 手机副本补发 | `a_staged_phone_copy_is_spent_once_the_peer_has_it` | 队列命中后走 `POST /upload`，请求里是 `filename="IMG_2043.jpg"` 与 multipart 的 `fromId`，落定后副本消失、队列清空 |
| 既有排队用例 | `a_queued_text_goes_out_when_the_peer_answers` 等 5 条 | 仍通过：离线不阻塞、等待不算失败、未被发现的设备不探测、来源消失即放弃、队列满丢最旧 |
| 队列溢出回收 | `the_oldest_queued_send_is_the_one_dropped_when_the_queue_fills` | 301 条压进 300 的窗口：`enqueue` 返回被挤出的那一条（`message_id` 正是最旧的），调用方据此删掉它的暂存副本；副本释放后 oldest 消失、newest 仍在，最旧气泡同时标失败 |
| 前端类型检查 | `npx tsc --noEmit` | 通过（`chat_send_files` 改返回 `SendOutcome`，新增 `stageFile`） |
| 暂存驱动（真实 `stageFile`） | dev 预览页 stub `__TAURI_INTERNALS__.invoke` 后调用 | 1.2 MB → 3 块（524288/524288/151424），逐块 base64 解回拼接与原字节完全一致；中途写失败触发 `chat_stage_cancel` 并抛出错误；0 字节文件不产生 append 调用 |
| 离线手机发送全链路 | 同上，模拟选择 100 B 的「合同扫描件.pdf」 | 命令序列 `chat_stage_open → chat_stage_append → chat_send_files(["…/queued/stage-1/合同扫描件.pdf"])`，toast「1 个文件已排队，对方上线后自动发送」，无错误条 |
| 排队 UI | dev 预览页 390×844 | 列表「待发送」、「2 条内容待 客厅的电脑 上线后自动发送，可以离开这一页。」、2 条 `.bubble.queued`（窄屏 opacity 0.62 + 实线 + 「待对方上线」） |

### 本轮仍未覆盖（设备已拿走，明天真机验证）

- 真机上「对方重新出现 → 5s 内自动补发」的实测：mDNS 重新广播、前台服务在切后台/锁屏后仍驱动
  `outbox` 节拍，都只在桌面与 dev 预览里证明过。
- 分块暂存在 Android WebView 里的实际吞吐与内存占用。512 KB/块是拍脑袋的保守值，
  几百兆的视频没在真机上跑过；暂存副本会占用应用私有目录，直到对方收到或超过 7 天才释放。
- 手机发出的排队文件在对方（电脑/另一台手机）落盘后的显示与打开。
- 网页设备仍然收不到文件：`chat_send_files` 对 browser 会话直接返回「暂时只能接收文字」，
  排队分支也不会为它生成条目。这是能力缺口，不是本轮漏做——浏览器页只能由 WS 心跳判定在线，
  补发需要有把字节推给页面的通道，目前没建。
- 满 300 条队列、7 天 TTL 到期的真机行为（逻辑有单测，但没有 Android 端的重启/清理实测）。

## 手机端文件卡片信息被截断（本轮，2026-11-07 00:50–01:10）

反馈：手机对话里的文件气泡只显示「季度…」，PC 端同一消息显示完整的「季度报告.xlsx」+ 大小。

根因（实测，非猜测）：窄屏规则 `.app.is-narrow .bubble.file { width: min(280px, 74%) }` 的百分比没有
可解析的包含宽度——消息行是收缩包裹的，于是宽度退回 `auto`，气泡实际只有 109px；再叠加
`.file-name { overflow:hidden }` 让它的最小内容宽度算作 0，卡片一路塌到只剩图标，名字只剩 39px，
七个字截成了两个半字。

改法：窄屏文件卡片给确定宽度 `268px`（`max-width: 100%` 兜底小屏），文件名不再单行硬截，
改为微信式的两行折行（`-webkit-line-clamp: 2`），两行放不下才省略。规则只在 `.app.is-narrow` 下，
宽屏一行 `nowrap` 的原有表现完全不变。

| 项目 | 尺寸 | 结果 |
| --- | --- | --- |
| 修复前 | 390×844 | 气泡 109px、卡片 84px、名字可用 39px / 需要 77px → 截断 |
| 修复后（短名） | 390×844 | 气泡 268px、卡片 243px、名字可用 198px，「季度报告.xlsx」完整，1 行 |
| 修复后（长名） | 390×844 | 「2026年度第四季度经营分析报告（终稿）.xlsx」需要 262px → 折 2 行（高 38px）完整显示 |
| 极端长名 | 390×844 | 80 个 A + 中文后缀：稳定停在 2 行，超出部分 clamp 省略，不撑破卡片 |
| 排队中的文件卡片 | 390×844 | 仍 268px，meta 位显示「待对方上线」，气泡 opacity 0.62 不变 |
| 宽屏回归 | 1180×820 | 卡片 240/315/457px、`white-space: nowrap`、单行，与修复前一致 |

验证方式：dev 预览页（vite :1420）stub `__TAURI_INTERNALS__`（含 `metadata.currentWindow`，
否则 `getCurrentWebview()` 抛错整棵树卸载）后挂载真实 `App`，`innerWidth` 覆写为 390 / 1180 两种尺寸，
读 `.file-card` / `.file-name` 的 `getBoundingClientRect()`、`scrollWidth` 与 `scrollHeight`。

# 2026-11-07 01:00–02:35 安卓四项反馈修复（logo / 发文件失败 / 键盘遮挡 / 切后台离线）

设备已被拿走，本轮只做开发与构建层面的验证，四项的运行时表现全部留给明天真机。

## 一、启动图标仍是默认 logo

根因不是没生成图标：`src-tauri/icons/android/**` 早在 19:58 就由 `tauri icon FileDrop-1.png` 生成过，
但 `gen/android/app/src/main/res/` 里还是 `tauri android init` 当时拷进去的默认资源——项目里连
`mipmap-anydpi-v26/` 目录都不存在，五个密度的 `ic_launcher*.png` 也仍是默认图。图标只在 `init`
时同步一次，之后重新生成不会自动跟过来。

做法：重跑 `npx tauri icon FileDrop-1.png`，把 5 个密度目录（`ic_launcher` / `ic_launcher_round` /
`ic_launcher_foreground` 共 15 张）、`mipmap-anydpi-v26/ic_launcher.xml`、`values/ic_launcher_background.xml`
全部同步进 `gen/android`，逐张 md5 比对一致。

顺带实测出一个会被遮罩暴露的问题（PowerShell System.Drawing 逐像素采样）：源图 1115×1115 的圆角之外是
透明（A0），渐变从左上 `#04E16F` 到右下 `#2CB96A`；`tauri icon` 采到透明角就把自适应图标的背景生成为
`#FFFFFF`。方形/squircle 遮罩（荣耀、三星都是）会在四角露出白边。于是新增
`drawable/ic_launcher_background.xml`（315° 线性渐变 `#FF04E16F → #FF2CB96A`），让
`mipmap-anydpi-v26/ic_launcher.xml` 的 background 指向它；`values/ic_launcher_background.xml`
（含 `icons/android` 源文件）同步改成 `#18CD6C` 作为回退，将来若有人重跑 `tauri icon` 也不会退回白色。

| 项目 | 结果 |
| --- | --- |
| 资源同步 | 15 张 PNG + anydpi XML + 背景色，源与工程 md5 全部一致 |
| APK 内实际字节 | AAPT2 会压缩路径并 crunch PNG，`res/mipmap-xxxhdpi/…` 在包里已不存在，改按 `aapt2 dump` 取证；下列结果在 01:21 与第五节重跑的 02:29 两个包上各做过一遍，一致 |
| 前景图 | 包内 `res/as.png` md5 `ade599a87d491c363dffaeab57242a77`，与 `icons/android/mipmap-xxxhdpi/ic_launcher_foreground.png` 逐字节相同 |
| 自适应图标 | `aapt2 dump xmltree --file res/BW.xml`：foreground `@0x7f0d0001`、background `@0x7f070093` |
| 背景层 | `aapt2 dump resources` 把 `0x7f070093` 解析为 `drawable/ic_launcher_background`（`res/0w.xml`）；`dump xmltree --file res/0w.xml`：315° 线性渐变 `#ff04e16f → #ff2cb96a` |
| 桌面显示 | 待明天真机（荣耀 MagicOS 的遮罩形状） |

## 二、安卓版发送文件失败

根因：手机端发送走的是 webview 里的 XHR（`bridge.ts` 的 `uploadFile`）。Tauri Android 的页面源是
`https://tauri.localhost`，向 `http://<peer>:8787/upload` 发请求同时撞上混合内容拦截与 Android 明文流量策略
（release 构建 `usesCleartextTraffic=false`），XHR 直接 `onerror`，于是弹出「无法连接到对方，检查地址与网络」。
桌面端没有这个现象，因为它由 Rust 直接建 socket。

做法：手机端不再在 webview 里发任何请求。选择文件 → `stageFile()` 以 512 KB base64 分块落进
`<app_config_dir>/queued/<uuid>/<原文件名>` → 交给桌面同一条 `chat_send_files`；对方在线立刻发、离线进队列，
字节全部由 Rust 推送。随之删除 `uploadFile`、`LocalIdentity`、`api.noteSentFile`、`newLocalId` 以及 Rust 侧
的 `chat_record_file` 命令（`invoke_handler` 注册表同步移除）。

| 项目 | 结果 |
| --- | --- |
| 旧文案出处 | 全仓 `grep 无法连接到对方` 已无命中，错误串随 XHR 一起删除 |
| Rust | `cargo test --lib` 25/25 |
| 前端 | `npx tsc --noEmit` 通过 |
| 打进包的代码 | arm64 APK 内嵌 `index-Czr2Vwy1.js`，与本轮 `dist/assets` 同名 |
| 真机发送 | 待明天（手机→电脑、离线排队补发） |

## 三、输入法挡住会话输入框

根因：`MainActivity` 调用了 `enableEdgeToEdge()`（等价 `setDecorFitsSystemWindows(false)`），targetSdk 37，
这种窗口在键盘弹出时不再被系统缩小，`windowSoftInputMode="adjustResize"` 已经不起作用，webview 高度不变，
输入框就留在键盘下面。

主改法（原生）：`MainActivity.keepComposerAboveKeyboard()` 在 decorView 上监听 WindowInsets，把
`Type.ime()` 的底部高度作为 padding 加到根视图，并把这段 inset 消费掉后再下发（`insets.inset(0,0,0,ime)`），
避免页面又用 `env(safe-area-inset-bottom)` 补一次。webview 因此被真正缩小——正是过去 adjustResize 对整个窗口做的事。

兜底改法（页面）：App.tsx 继续测量 `visualViewport`，把被盖住的高度写进 `--keyboard`，
`.app { height: calc(100dvh - var(--keyboard, 0px)) }`。原生缩小生效时该值算出来是 0，两条路径不会叠加。

连带问题：窗口缩小后最后一条消息会被留在键盘腾出的空间里。ChatView 增加跟随逻辑——只有本来就在读最新消息的人
才重新钉回底部，翻历史的人保持原位；它同时监听 window 与 visualViewport 的 resize，分别对应「窗口被缩小」和
「键盘只覆盖不缩小」两种形态。

dev 预览实测（390 宽、真实视口高 734，用覆写 `visualViewport.height` 模拟键盘只覆盖 336px）：

| 场景 | `--keyboard` | `.app` 高 | 输入框底边 | 消息区 scrollTop / 最大值 | 最后一条消息 |
| --- | --- | --- | --- | --- | --- |
| 键盘收起 | 0px | 734 | 734 | 3 / 4 | 可见 |
| 键盘弹出 | 336px | 398 | 398 | 339 / 340 | 可见（自动钉回底部） |
| 先翻到顶部，再收起→弹出 | 0 → 336px | 734 → 398 | — | 保持 0 | 不被拽走 |
| 回到顶部位后再弹出（已贴尾） | 336px | 398 | — | 339 / 340 | 钉回底部 |

Kotlin 侧只证明编译通过（APK 构建成功、四个包全部产出），真实键盘行为待明天。

## 四、切到其他应用就提示不在线

根因：前台服务其实从来没起来过。开机自启时 `start_transfer` 在 `setup` 阶段执行，那一刻 Android 侧的
webview 还不存在，而 `start_background_service` 必须通过 webview 的 JNI handle 把 intent 投递到 UI 线程，
于是直接失败。进程一被冻结（Android 12+ 会冻结缓存进程），mDNS 与 socket 全停，别的设备自然就看不到它了。

做法三处：
- `keep_resident()`：起服务失败时每 500ms 重试，最多 60 次；期间若传输已停止就退出，不再打扰。
- 新增 `transfer_keep_alive` 命令，前端 `visibilitychange` 回到前台时再声明一次服务——系统可能在应用离开期间
  回收它，而服务器仍在监听，不重新声明就会一直「不在网」。
- `ServerService.onStartCommand` 收到 null intent（进程被杀后的 START_STICKY 复活）时直接 `stopSelf()`：
  那时 Rust 侧的服务器并不在运行，继续挂着「正在接收」的通知与 wake lock 等于谎报在线还白耗电。

| 项目 | 结果 |
| --- | --- |
| Rust / 前端 | `cargo test --lib` 25/25、`tsc --noEmit` 通过 |
| 命令注册 | `transfer_keep_alive` 已进入 `invoke_handler`，`chat_record_file` 已移除 |
| 编译 | 四个 ABI 的 APK 全部构建成功（含 Kotlin 改动）；第五节又整批重跑了一次，02:29 的包已复核 |
| 后台可达 | 待明天真机：切走 5 分钟后另一台设备能否发现并发送 |

## 五、补做：`dataSync` 的 6 小时上限

上一节把服务真正拉起来之后，接着把它自己的坑填了：`dataSync` 在 Android 14+ 有「任意 24 小时内累计 6 小时」的
配额，超了系统直接停服务——本机随即从局域网消失，正好是用户报的那个症状，只是换了个触发条件（挂着半天再切过去
就会复现）。对一个「随时能被发文件」的接收端，这个类型本身就是错的。

改法：

- 清单同时声明 `specialUse|dataSync`，并补 `FOREGROUND_SERVICE_SPECIAL_USE` 权限；`dataSync` 那条权限保留，
  因为低版本仍走它。`<service>` 里加 `PROPERTY_SPECIAL_USE_FGS_SUBTYPE` 说明用途（API 31+ 才读，24 忽略未知标签）。
- `ServerService.foregroundType()`：API ≥ 34 返回 `FOREGROUND_SERVICE_TYPE_SPECIAL_USE`，否则 `DATA_SYNC`。
  类型必须是当台设备认识的那个，不能一律写死。
- 三参 `startForeground` 是 API 29 才有的重载，minSdk 24 直接调会在 8.x 上 `NoSuchMethodError`；因此 29 以下
  退回两参版本，服务类型只由清单声明。

包内取证（`aapt2 dump`，对象是 02:29 重新构建的 `app-arm64-release.apk`）：

| 项目 | 结果 |
| --- | --- |
| 权限 | `FOREGROUND_SERVICE_SPECIAL_USE` 与 `FOREGROUND_SERVICE_DATA_SYNC` 同时存在 |
| 服务类型 | `foregroundServiceType(0x01010599)=0x40000001`，即 SPECIAL_USE(0x40000000) \| DATA_SYNC(0x1) |
| 用途声明 | `android.app.PROPERTY_SPECIAL_USE_FGS_SUBTYPE` 已进包，值为本机说明文本 |
| Kotlin | 四个 ABI 的 gradle 构建全部成功（`foregroundType()` 改动参与编译） |
| 图标未受影响 | `res/as.png` md5 `ade599a87d491c363dffaeab57242a77`，与 `icons/android/mipmap-xxxhdpi/ic_launcher_foreground.png` 相同；`res/0w.xml` 仍是 315° `#ff04e16f → #ff2cb96a` |
| 前端未受影响 | `lib/arm64-v8a/libfiledrop_lib.so` 内嵌 `index-Czr2Vwy1.js`，与 `dist/assets` 同名 |
| 6 小时配额 | 无法在编译期证明，明天真机挂满 7 小时以上再看向网络 |

## 本轮构建

| 产物 | 时间 | 体积 |
| --- | --- | --- |
| `app-arm64-release.apk` | 02:29 | 9.50 MB |
| `app-arm-release.apk` | 02:33 | 6.96 MB |
| `app-x86-release.apk` | 02:31 | 10.11 MB |
| `app-x86_64-release.apk` | 02:27 | 10.12 MB |
| `FileDrop_1.0.0_x64-setup.exe` | 01:31 | 2.11 MB |
| `FileDrop_1.0.0_x64_en-US.msi` | 01:31 | 2.95 MB |

Windows 包已静默装到 `AppData\Local\FileDrop`（装前 `tasklist` 确认无实例在跑）。安装的 exe 大小 5,902,848、
时间 01:31:14，且二进制里 `grep -a -o "index-[A-Za-z0-9_-]*\.js"` 只命中 `index-Czr2Vwy1.js`，与本轮
`dist/assets` 及构建产物一致——装进去的确实是新包，不是上一次留下的文件（md5 与 `release\filedrop.exe`
不同属正常：NSIS 打包前会给 exe 打上 bundle 类型标记）。桌面端需要重启 FileDrop 才生效。

四个 ABI 的包只会在 `tauri android build --apk --split-per-abi` 下产出；漏掉 `--split-per-abi` 时 CLI 只写
`outputs/apk/universal/`（四个 `.so` 全塞进一个 30.5 MB 的包），而且 `--target aarch64 armv7 …` 这种多目标写法
同样只出 universal，`--target` 接逗号分隔会直接报错。这一点是被 01:37 / 02:12 / 02:21 三次「构建成功但 01:20 的
arm64 包纹丝不动」抓出来的——那几次的 exit code 都是 0，只有 stat 输出目录才发现包没换。
`outputs/apk/universal/app-universal-release.apk`（02:21，10.12 MB，单 x86_64）就是那次留下的中间产物，别当成交付物。

收尾还顺手清了 Android 侧的三条编译告警：`tauri::WindowEvent` 只在 `#[cfg(desktop)]` 块里用、`shell::VERB_KEY`
与 `VERB_LABEL` 只在 Windows 注册表代码里用，都补了 cfg 门。之后 `cargo check --target x86_64-linux-android`
与桌面端 `cargo check --release` 均无告警。这两处不改变任何行为，01:31 装好的 Windows 包不必重装。

## 本轮仍未覆盖 / 已知缺口

- 明天真机要看的四件事：荣耀桌面上的图标形状；手机→电脑发文件（在线与离线排队各一次）；键盘弹出时输入框是否
  真的抬到键盘上方；切后台/锁屏后本机能否继续被局域网发现。装到手机上的应该是 02:29 的
  `app-arm64-release.apk`（`--split-per-abi` 那批），早于 02:25 的任何包都还带着旧的 `dataSync` 清单，不能用。
- 常驻服务已从 `dataSync` 换成 `specialUse`（见第五节），6 小时配额这条不再成立；代价是它属于「需要向审核解释」
  的类型——直接安装 sideload 的 APK 不受影响，真要上架 Google Play 得在商店列表里写明理由，那条
  `PROPERTY_SPECIAL_USE_FGS_SUBTYPE` 说明文案就是为此准备的。
- 荣耀/小米等 OEM 的省电策略仍可能直接杀掉后台进程，需要在系统设置里允许自启动或关闭电池优化——这不是应用能强制的。
- 网页设备仍然收不到文件（能力缺口，见上一节），本轮未动。
- 暂存副本在对方收到之前一直占用应用私有目录；超过 7 天或队列满 300 条会丢弃最旧的一条并连带删除它的副本。

---

# 2026-11-07 03:00–03:40 安卓两项反馈：默认设备名 / 保存位置改不了

设备仍不在手上，本轮只做开发 + 能在本机拿到的证据，真机项一律单列。

## 一、装完启动，设备名是默认的 FileDrop

根因是两条叠加，不是「没读」：

1. `default_device_name()` 走 `COMPUTERNAME` → `HOSTNAME` → `/proc/sys/kernel/hostname`。安卓应用进程三条都读不到
   （`/proc/sys/kernel/hostname` 被 SELinux 拒绝），于是落到 `FALLBACK_NAME = "FileDrop"`。
2. 配置里一旦写下 `FileDrop`，`Config::load` 的「存过的名字优先」就把它永久保下来——即使以后读得到真名也不会换。
   而一般人不会去改这个名字，所以局域网里所有手机都叫 FileDrop，等于认不出设备。

做法：

- 安卓分支先问系统属性，顺序 `ro.product.marketname` → `ro.config.marketing_name` → `brand + " " + model`，
  用 `__system_property_get` FFI（这些是公开属性，应用进程可读），并过滤空串、`unknown`、`null`。
- `resolve_device_name()` 只把「存下来的名字恰好是占位名 FileDrop」换成派生出的真名；派生不出真名时保持原样。
- 桌面端不做这个替换：桌面一直读得到主机名，那里的 `FileDrop` 是用户自己的选择。

**取舍（本轮的裁定，不是遗留问题）**：如果用户真的主动把手机设备名改成 `FileDrop`，升级后会被换成一次真实机型名，
换完即固化到配置。这是为了让绝大多数「从没改过名字」的设备可识别而付的代价。

证据：`cargo test --lib` **28 passed**，其中三条是本轮新写的——
`a_placeholder_name_gives_way_to_a_real_one`、`a_name_somebody_chose_is_left_alone`、
`a_blank_stored_name_falls_back_to_the_derived_one`；`cargo check --target x86_64-linux-android` 通过且 filedrop crate
0 告警。`__system_property_get` 的符号能在 NDK 链接下解析，是之前单独跑一次 `cargo build --target aarch64-linux-android`
（补 `CARGO_TARGET_*_LINKER`/`CC_*`/`AR_*`）证明过的，不是推断。

## 二、「我的 → 设置」里的文件保存位置无法修改

根因不在后端：`transfer_set_config` 从来就接受任何非空目录。是前端把这个能力关掉了——
移动端分支把「保存位置」渲染成只读 `<span>`，`pickFolder()` 第一行就 `if (status.mobile) return;`。

但直接放开会得到「改了却不生效」的更坏结果：Android 10+ 的 scoped storage 下，应用不能按路径在
`/storage/emulated/0/Download` 里创建文件，`create_dir_all` / `File::create` 会被拒。所以本轮做的是「能真的落进去」：

- 新增 Kotlin `StorageBridge`（Rust 经 JNI 调用，沿用工程里已有的 `with_webview` + `exec` + channel + 3s 超时 pattern）：
  - API ≥ 29 且目录在共享存储下 → 往 `MediaStore.Downloads` 插一行（`DISPLAY_NAME` + `RELATIVE_PATH` + `MIME_TYPE`），
    读回 `_data`；行是空的、由 Rust 侧把字节流进这个自己拥有的文件（应用写自己的文件平台是允许的）。
    查不到路径的行当场删除，不留一条相册里的假记录。重名不算失败：MediaStore 会自动改名，改名后的路径就是返回值。
  - API ≤ 28、或目录不在共享存储（别的卷、10 以下）→ 直接文件系统，重名自动加 `(1)`…`(999)`。
- Rust 侧统一成一个 `landing_path()`，同时接管两个写盘点（WebSocket 头里的直传、`/upload` 的 multipart 逐字段），
  原来各自的 `create_dir_all` 前置检查删掉了。**认领失败或路径不可写就回落到应用私有目录**——文件一定落地，
  只是换个地方，不存在「改了保存位置之后收不到文件」。
- 手机端「我的 → 保存位置」现在是一个能点的行，展开选择面板：下载 / FileDrop（推荐）、下载、下载下的自定义子文件夹、
  应用私有目录。候选路径由新命令 `transfer_storage_roots` 给出（JNI `Environment.getExternalStorageDirectory()`
  + app data dir），界面显示「下载 / xxx」这种人话，完整路径留在 `title`。子文件夹名过滤 `\ / : * ? " < > |`。
- 「打开保存位置」在安卓改成 `DocumentsContract` + `ACTION_VIEW`（MIME `vnd.android.document/directory`）调起系统
  文件浏览；没有应用能显示时返回一句「这台手机没有可以显示该文件夹的应用，文件保存在：…」，而不是假装成功。
- 清单补 `WRITE_EXTERNAL_STORAGE`（`maxSdkVersion=28`），`MainActivity` 在 API ≤ 28 运行时申请；它和 13+ 的通知权限
  互斥，同一次启动最多弹一个框。

证据（今天能拿的，全部实测）：

| 检查 | 结果 |
| --- | --- |
| `npx tsc --noEmit` | 无输出（通过） |
| `./gradlew :app:compileArmReleaseKotlin` | 通过（`StorageBridge.kt` / `MainActivity.kt` 可编译） |
| 浏览器 mock（390×844 + `mockStatus.mobile=true`）保存位置行 | 初始显示「应用私有目录」，与 mock 的 `privateDir` 正确匹配 |
| 点选「下载 / FileDrop」 | 行文案变「下载 / FileDrop」，`title` 为 `/storage/emulated/0/Download/FileDrop`，面板自动收起 |
| 自定义子文件夹输入 `工作/报告 2026` | 实际写入值被过滤成 `工作报告 2026`（证明过滤代码真的跑了），选中后 `title` 为 `/storage/emulated/0/Download/工作报告 2026` |
| 选项行几何 | 3 个选项各 49px 高、左边 12px 内衬，面板右边界在 390px 根容器内（不溢出） |
| 文字层级 | 标题 15px/500 `#191919`，说明 11.5px `#888`，选中勾 `#07c160` |
| 展开箭头 | 计算后 `matrix(0, 1, -1, 0, 0, 0)`（90°） |

箭头这条值得一提：最初把 `open` 类加在 `<svg>` 根上，这台 Chrome 里 `getComputedStyle().transform` 恒为
`matrix(1, 0, 0, 1, 0, 0)`，连内联 `style.transform = "rotate(90deg)"` 也一样，即 CSS 对 svg 根的 transform 没生效；
改成由外层 `<span class="me-chev">` 承载后立刻正确，同时也就避开了旧版 WebView 的同类问题。

只能真机证明的（明天）：

- MediaStore 插行到底给不给 `_data`。荣耀/华为等定制 ROM 对非媒体文件可能不给路径。代码里
  `claimed_is_writable()` 是这道风险的探针：路径打不开就回落私有目录，所以最坏情况是「文件在私有目录」，不是丢文件；
  但「能不能真的落进下载目录」这条必须装到手机上发一个文件才知道。
- `ACTION_VIEW` 打开文件夹在荣耀文件管理器上的表现（可能没有任何应用接管，此时是那句提示文案）。
- API ≤ 28 老机器上的存储权限弹窗，以及弹窗被拒后 `claimOnDisk` 的回落。
- 应用在后台时（Activity 已暂停、只剩常驻服务）`claim_landing_path` 还拿不拿得到 UI 线程：拿得到才谈得上「后台收到的
  文件落进下载目录」，拿不到就按设计回落私有目录。这条同样只会表现为「文件换了个地方」，不会表现为丢文件。
- 设备名：真机上看它是否显示成机型市场名（例如 `HONOR …` 而不是 `FileDrop`）。

已知的行为变化（不是缺陷，但要知道）：走共享存储时，文件在**开始接收**时就会在下载目录里出现一条空记录，
传完才有内容；中途断掉会留下半个文件（和桌面端一样，只是现在手机上的相册/文件管理器能看见它）。

截图缺口：应用内 Browser 面板 `visibilityState=hidden`，`take_screenshot` 报
`NATIVE_BROWSER_VIEWPORT_UNAVAILABLE`，本轮以计算样式 + 几何盒子为证；像素级复核并入明天的真机清单。

## 三、R8 把「只有 Rust/JNI 引用」的类裁成空壳（本轮真正的坑）

03:22 那批 APK 打出来是 exit 0、Kotlin 编译通过、`classes.dex` 里也能搜到类名——但把 dex 反汇编出来看，
`Class #551 Lcom/filedrop/app/StorageBridge;` 是 `PUBLIC FINAL` 且**静态字段、实例字段、直接方法、虚方法四段全空**：
类壳留着，`claim` / `externalRoot` / `openFolder` 三个 `@JvmStatic` 方法被 R8 全部裁掉了。

也就是说：这一节（保存位置）如果只凭「编译通过 + APK 生成」交付，装到手机上会表现为**功能存在但完全无效**——
Rust 侧 `GetStaticMethodID` 取不到方法，认领失败，每个文件都静静落进应用私有目录；用户改了保存位置，界面上什么都对，
文件却永远不在下载目录。而且这条路径不会崩、不会红、不会有任何日志，是典型的「绿构建抓不到的死路径」。

为什么以前没暴露：上一节（11-06 的 R8 风险备注）静态确认过当时的结论是对的——那时 Rust 侧 JNI 只碰
`android.content.Intent` / `android.net.Uri` / `androidx.core.content.FileProvider` 这些框架类，
自己的类只有 `MainActivity` / `ServerService`，都由清单引用而被 `aapt_rules` 保留，且只按类名启动服务、不调具体方法。
`StorageBridge` 是第一个「只被 JNI 调用、清单里也没有」的应用类，正好落进保留规则的空白区。
wry 自动生成的 `proguard-wry.pro` 对本包只写了 `-keep class com.filedrop.app.* { native <methods>; }`，
而 `StorageBridge` 没有 native 方法，所以它一条也接不上。

做法：在工程自己的 `app/proguard-rules.pro`（`build.gradle.kts` 用 `**/*.pro` 通配挂进来，不是自动生成文件）加
`-keep class com.filedrop.app.StorageBridge { *; }`，注释里写清这条是功能开关而不是优化项。
**验收标准也一并改了**：不看构建退出码，只看重打包后的 `dexdump` 里 `StorageBridge` 的 `Direct methods`
是否列出 `claim` / `externalRoot` / `openFolder` 三个描述符（`(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;` 等）。

补规则后重打包（03:36–03:39）的实测结果，arm64 逐个方法核对：

| 成员 | 类型描述符 | flags |
| --- | --- | --- |
| `claim` | `(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;` | `PUBLIC STATIC FINAL`，带 code |
| `externalRoot` | `()Ljava/lang/String;` | `PUBLIC STATIC FINAL`，带 code |
| `openFolder` | `(Landroid/content/Context;Ljava/lang/String;)Z` | `PUBLIC STATIC FINAL`，带 code |
| `claimThroughMediaStore` / `claimOnDisk` | 同上私有实现 | 一并保留 |
| `EXTERNAL_STORAGE_AUTHORITY` | `Ljava/lang/String;`，值 `com.android.externalstorage.documents` | 常量仍在 |

三个方法都是 `STATIC`，正是 Rust `call_static_method` 需要的形式（只证明类名在 dex 里是不够的，必须看到 static + 描述符 + code）。
`arm` / `x86` / `x86_64` 三个包同样搜到 `openFolder` 与 `claimThroughMediaStore`，说明保留规则对四个 flavor 都生效。
顺带用 `aapt2 dump badging` 复核了新清单：`minSdk 24 / targetSdk 37`、`FOREGROUND_SERVICE_SPECIAL_USE`、
`WRITE_EXTERNAL_STORAGE maxSdkVersion='28'` 都在包里。

一个不要误判的细节：新旧两批的 arm64 包体积按字节数完全相同（9544467），但两份 dexdump 的类成员段落一个是全空、
一个是满的——所以**体积相同不代表内容相同，也别用体积当验收**，这正是退出码和文件大小共同的盲区。

证据链（可复现的命令）：`unzip -o -q app-arm64-release.apk classes.dex` → `build-tools/34.0.0/dexdump -d classes.dex`
→ `grep -n StorageBridge`；`grep -a -o -E "Lcom/filedrop/app/[A-Za-z0-9_]*;"` 用来列包内类名（不加 `-a` 对二进制 dex 会静默无输出）。

## 四、本轮构建

| 产物 | 时间 | 体积 |
| --- | --- | --- |
| `app-arm64-release.apk` | 03:36 | 9.54 MB |
| `app-x86_64-release.apk` | 03:36 | 10.17 MB |
| `app-arm-release.apk` | 03:37 | 6.99 MB |
| `app-x86-release.apk` | 03:39 | 10.16 MB |
| Windows `setup.exe` / `msi` | 仍是 01:31 | 2.11 MB / 2.95 MB |

- 明天要装的是 **03:36 之后**这批。03:22–03:30 那四个包同样是本轮代码打的、也全部 exit 0，但里面的
  `StorageBridge` 是空壳，「保存位置」等于没做，不要用。
- Windows 没有重新打包：本轮对桌面端只有两类改动——`transfer_open_dir` 多了 `AppHandle` 参数（桌面分支行为不变）、
  设置面板桌面分支删掉一句永远走不到的三元文案。这两处分别由 `cargo test --lib`（28 passed，含三条新测试）和
  `npx tsc --noEmit` 覆盖，01:31 装好的包不必重装。
- dev 预览用的 Vite 在 APK 构建期间被 gradle 的中间产物（`merged_jni_libs/.../libfiledrop_lib.so`）以
  `EBUSY` 打死，属于两个构建同时写一棵树的副作用，不是应用缺陷；它自己退出，没有影响打包结果。

## 五、明天真机要做的两项（本轮新增的验收动作，按顺序做）

1. 保存位置：先在「我的 → 保存位置」选「下载 / FileDrop」并保存 → 从另一台设备发一个文件过来 →
   用手机文件管理器看 `下载/FileDrop` 里有没有它。
   - 在 → MediaStore 认领这条路成立，本节全部有效。
   - 不在，但应用私有目录里有 → 说明这台 ROM 不给 `_data` 或不允许按路径写，回落逻辑按设计接住了（文件没丢），
     这时要抓 logcat 里 `[filedrop]` 与 JNI 相关的一行来判断是哪一种，而不是直接改代码。
   - `adb logcat -s tmansi:*` 之外，重点看有没有 `NoSuchMethodError` / `ClassNotFoundException` 提到 `StorageBridge`
     ——那是保留规则没生效的信号，属于打包问题而不是存储问题，两者不要混。
2. 设备名：卸载重装（不是覆盖安装）后第一次打开，「我的」顶部与另一台设备的列表里都应当显示机型市场名
   （例如 `HONOR …`），不再是 `FileDrop`；随后手动改名并重启，改的名字必须保留。

---

# 2026-11-07 12:10–13:0x 真机全链路验证（HONOR ALI-AN00 / 荣耀X50 / Android 15 / arm64-v8a）

设备通过无线 ADB 连接（`192.168.0.37:37895`），安装的是 03:36 那批 `app-arm64-release.apk`
（安装后 `lastUpdateTime` 12:12:24，覆盖安装，保留原有配置与聊天记录——这正是要验的升级路径）。

## 一、环境限制（先记下来，后面所有结论都受它影响）

- **logcat 被 ROM 禁用**：`adb logcat -d` 输出为空（不是过滤掉，是整个缓冲区拿不到）。HONOR 零售机型的常规限制。
- **bugreport 里的 SYSTEM LOG 同样为空**：`bugreport-ALI-AN00-…12-15-25.txt` 的 `SYSTEM LOG` 段只有标题和耗时，
  没有任何日志行。
- 因此崩溃取证只能靠 `adb shell dumpsys activity exit-info` 与 `adb shell dumpsys dropbox --print`
  ——两个命令都能读，而且足够定位（见第三节）。
- `run-as` 不可用（release 包 `debuggable=false`），无法直接读写应用私有目录里的配置文件。
- **同一台手机上还有别的会话在操作**（idexnote 的悬浮「边缘把手」叠加层、以及 HORIZON Meeting 被误触启动）。
  多次 `input tap` 落到别的应用上，触摸类验证必须在独占时重做；这也解释了早期几次「点了没反应」。

## 二、升级后第一眼（已验证）

| 检查 | 结果 | 证据 |
| --- | --- | --- |
| 安装与冷启动 | 成功，7 秒内 `topResumedActivity` 与 `pid` 都不变 | `pid=26845` 前后一致 |
| 设备名 | **荣耀X50**（旧配置里存的是 `FileDrop`，升级后被机型市场名替换） | 接收页「设备名称」截图；`ro.product.marketname` 回填生效 |
| 接收服务 | `http://192.168.0.37:8787`，二维码正常，接收端口 8787 | 接收页截图 |
| 局域网发现（手机→电脑） | 「发现设备」列出 **X-BOOKS 192.168.0.27:8787**，在线绿点、标「已有对话」 | 发现页截图 |
| 聊天记录 | 对话列表仍能看到 X-BOOKS / FileDrop / X-BOOKS-PROBE 及其最后一条文件消息 | 对话页截图 |
| 保存位置 | 旧配置仍是应用私有目录（`/data/user/0/com.filedrop.a…`），这是旧默认值，见第四节 | 接收页截图 |

## 三、点「我的」整页崩溃（本轮真机抓到的头号问题）

现象：点底部「我的」→ 整页消失、回到上一个应用；进程退出。`dumpsys activity exit-info` 连续 8 条记录：

```
process=com.filedrop.app reason=4 (APP CRASH(EXCEPTION)) subreason=0 (UNKNOWN)
```

`dumpsys dropbox --print` 里对应条目的异常正文只有一行：

```
java.lang.NoClassDefFoundError: Class not found using the boot class loader; no stack trace available
Crash-Handler: org.chromium.base.JavaExceptionReporter
```

这条信息的形状本身就是根因线索：

1. **`FindClass` 找不到应用自己的类**。工程里所有 Android 调用都走
   `webview.jni_handle().exec(...)`，wry 把这段闭包交给它自己的 JNI 线程执行；该线程没有 Java 栈帧，
   `FindClass("com/filedrop/app/StorageBridge")` 只能按**引导类加载器**解析，于是返回
   `NoClassDefFoundError`，且 ART 在这种情况下给不出 stack trace（正好对应「no stack trace available」）。
   之前能一直工作的调用（`android/content/Intent`、`android/net/Uri`、`java/io/File`）全在引导类路径上，所以没暴露。
2. **异常没被清掉就逃逸**。「我的」页一挂载就调 `transfer_storage_roots` → `external_root` → 上面的查找失败；
   原代码里 `pending()` 只在成功路径上调用，失败时 Java 异常处于 pending 状态，一路冒进 Chromium 的 JNI 桥，
   `JavaExceptionReporter` 直接判死整个进程。所以「只是查一个路径」变成了整页崩溃。

同一个坑还埋着第二处：`open_file` 用 `FindClass` 调 `androidx/core/content/FileProvider.getUriForFile`
——androidx 也是应用类路径，这条「打开收到的文件」在真机上同样会崩，只是还没人点到。

修复（`src-tauri/src/android.rs`）：

- 新增 `app_class(env, context, name)`：用 `context.getClassLoader().loadClass(name)` 取类，返回 `GlobalRef`，
  再用 `call_static_method(&class, …)` 调用。`StorageBridge`（3 处）与 `FileProvider`（1 处）都改走它。
- 所有 6 个 JNI 闭包的返回统一成 `tx.send(pending(env, "无法完成操作").and(result))`：
  先检查并清掉挂起的 Java 异常，再回传结果。哪怕将来再出现类找不到，最多是这一条命令失败，不会连坐整个应用。

顺便裁定的两件事：

- **升级时的保存位置迁移**：旧默认值（应用私有目录）从来不是用户在界面上选的（那时根本没入口），
  所以配置里存着它时，加载时按「设备名占位符」同样的规则换成新的共享目录 `Download/FileDrop`；
  用户自己挑过的其它目录一律不动。`Config::load` 多了 `old_storage_default` 参数，
  Android 侧传入 `private_storage(app)`，桌面/iOS 传 `None`。
- 新增 3 条单测（旧私有默认值让位 / 挑过的目录不动 / 空值回落），`cargo test --lib` **31 passed**。

---

# 2026-11-07 13:0x 会话页四项反馈（文件卡片闪退 / 系统分享 / 头部「+」/ 返回键）

## 一、会话里点文件闪退

与第三节的「我的」崩溃同源：`open_file` 用 `FindClass("androidx/core/content/FileProvider")` 取类，
而 wry 的 JNI 线程只能看到引导类路径，取不到 → 挂起异常逃逸 → 进程被杀。
修复与「我的」一起做完了（`app_class()` + 每个闭包先清挂起异常），这一条要真机复测确认。

## 二、图库 / 文件管理器里没有「FileDrop」分享入口

根因：清单里 `MainActivity` 只有 LAUNCHER 过滤器，没有 `ACTION_SEND` / `ACTION_SEND_MULTIPLE`，
所以系统分享面板从来不会列出 FileDrop——不是分享失败，是这个入口压根没注册。

做法：

- 清单加两个 intent-filter（`*/*`，单发与多发）。
- `MainActivity` 在 `onCreate` / `onNewIntent` 里取 `EXTRA_STREAM`（单个 `Uri` 或列表），
  在后台线程把 `content://` 读出来复制到分享收件箱，重名自动加 ` (1)`；复制完调用页面钩子
  `window.__filedropShared()`。
- 收件箱里的文件是**一次性**的：新命令 `transfer_take_shared` 把它们移进 `inbox/handled/` 再返回新路径，
  所以重复启动或重复切前台不会反复弹发送面板，路径又始终可读（真正发送时还要用）。
  冷启动时页面挂载即调用一次；运行中分享则靠上面的 JS 钩子。收到的文件走的是既有的发送面板
  （与资源管理器右键「FileDrop 传送」同一个 `sendToPaths` 流程）。
- **路径只留一个出处**：收件箱位置由 Kotlin `StorageBridge.shareInbox(context)` 返回
  （`context.dataDir/share-inbox`），Rust 经 JNI 拿。踩到的坑：Tauri 的 `app_data_dir()` 在 Android 上是
  `dataDir` 而不是 `filesDir`（`PathPlugin.getDataDir` 就是 `activity.dataDir`），两边各自拼路径必然对不上，
  而且失败方式是「分享后面板永远不弹」这种静默错误。
- 顺带把 `StorageBridge` 的 4 个方法（含新的 `shareInbox`）都过一遍应用类加载器；保留规则已在上一轮加过。

## 三、会话头部「发送文件」改成「+」

手机（`narrow`）改成 34px 圆形「+」按钮，`title` / `aria-label` 仍是「选择文件发送给这台设备」；
桌面保留带文字的按钮（桌面有横向空间，文字更明确）。样式 `.chat-plus`，按下有浅灰底。

## 四、手势返回退到桌面，而不是回上一页

根因在 wry 自带的返回回调：

```kotlin
if (mWebView.canGoBack()) mWebView.goBack() else onBackPressed()   // → finish()
```

这是个单页应用，`canGoBack()` 永远为 false，于是从会话/「我的」/设置里返回都直接把 Activity 关掉，退回桌面。

做法：`MainActivity.onWebViewCreate` 里**追加**一个 `OnBackPressedCallback`（后注册的先执行），
先调页面钩子 `window.__filedropBack()`，页面按这个顺序处理：
设置面板 → 发送面板 → 会话详情 → 回到「对话」tab；什么都不剩时才返回 `false`，
这时把本回调禁用再 `onBackPressed()`，交回 wry 的默认行为（关窗）。

页面侧在 `App.tsx` 里注册该钩子，依赖 `settingsOpen / sendToPaths / chatOpen / active / tab`，
与界面上各处的返回按钮保持同一套语义。JS 结果异步返回，因此「关窗」在回调里触发；
真机上手势返回应当落在列表页而不是桌面。

---

# 2026-11-07 13:00–14:10 真机验证记录（第二批，装的是 12:44 / 13:09 两版包）

## 已经验过的（有截图或命令输出）

| 检查 | 结果 | 证据 |
| --- | --- | --- |
| 点「我的」不再崩溃 | 通过 | `topResumedActivity` 与 `pid` 前后不变（14630→14630），页面正常渲染 |
| 设备名 | 通过 | 「我的」顶部与接收页都显示 **荣耀X50** |
| 保存位置升级迁移 | 通过 | 「我的」显示 **下载 / FileDrop**，旧的应用私有目录已被替换 |
| 保存位置选择器 | 通过 | 展开后三项（下载/FileDrop ✓、下载、应用私有目录）+ 自定义子文件夹 + 说明文案，箭头翻转 |
| 手势/按键返回 | 通过 | 会话页 → BACK → 回到「设备」列表，应用没有退出（`topResumedActivity` 始终是 MainActivity） |
| 会话头部「+」 | 通过 | 右上角圆形「+」，点击仍是选文件 |
| 接收文件落进下载目录 | 通过 | `POST /upload` 返回 200「成功保存 1 个文件」，`/storage/emulated/0/Download/FileDrop/` 出现 34 B 文件；对话里同时出现该卡片 |
| 输入法 | 通过（独占时） | 点输入框后 `mInputShown=true`、`mServedView=RustWebView`，`input text test1` 真的出现在输入框里 |

## 这一轮修掉的两个新根因

**1）文件卡片打不开：FileProvider 没有覆盖接收目录。**
`private_storage()` 落在 `app_data_dir()/FileDrop`，而 Android 上 `app_data_dir()` 就是
`/data/user/0/<pkg>`（`PathPlugin.getDataDir` 返回 `activity.dataDir`），**不在 `files/` 下面**；
`file_paths.xml` 只配了 `<files-path>`、`<cache-path>`、`<external-*>`，所以 `getUriForFile` 找不到
匹配的 root 并抛 `IllegalArgumentException`。上一轮加的「先清异常」守卫把它变成了界面上的
「无法完成操作」——不再闪退，但文件依然打不开。已补 `<root-path name="app_data" path="." />`。

**2）MediaStore 行建了、文件却没走 MediaStore 通道。**
实测：`Download/FileDrop` 里有文件、但 MediaStore 只有目录行（`_id=1000166346`），没有文件行。
对照实验：用 shell `content insert` 往同一个 collection 插一行是成功的，说明 MediaStore 本身能用，
问题在应用侧。判定为插入了**合并卷** `MediaStore.VOLUME_EXTERNAL`——该卷不允许 insert，必须用
`VOLUME_EXTERNAL_PRIMARY`。已改，**这一条要等新包验证**（验证标准：发一个文件后
`content query … --where "relative_path='Download/FileDrop/'"` 能查到文件行且 `_size>0`）。

顺带修掉的两处：

- **远端地址被 IPv6 链路本地覆盖**。截图里出现过 `在线 · fe80::b646:…:8787`：mDNS 的某些应答只带 AAAA，
  代码原来会用它覆盖已有的 IPv4 host；`fe80::` 没有 zone 直连不通，于是表现为「对方离线→稍后又能发」。
  改为：同网段 IPv4 → 任意 IPv4 → 非链路本地 IPv6 → 都没有则**保留原 host**，不再用不可用地址覆盖。
- **JNI 失败文案改为按操作命名**（无法打开链接/文件/保存位置/读取分享目录/启动后台接收服务…），
  以后界面上再出现这类提示，就能直接看出是哪一步失败，而不是统一的「无法完成操作」。

## 系统分享（图库 → FileDrop）

- 清单已带 `SEND` / `SEND_MULTIPLE`（`aapt2 dump xmltree` 里各 1 条），安装后系统分享面板应当出现 FileDrop。
- `MainActivity` 在 `onCreate` / `onNewIntent` 取 `EXTRA_STREAM`，后台线程复制到
  `getExternalFilesDir(null)/share-inbox`（路径由 Kotlin `StorageBridge.shareInbox()` 单一给出，Rust 经 JNI 取），
  复制完调用页面钩子；页面挂载时与切回前台时各取一次，取到的文件走已有的发送面板。
- 用 `adb shell am start -a SEND --grant-read-uri-permission --eu …STREAM content://media/…` 模拟时，
  收件箱目录被创建但**副本没有被写出来**（`share-inbox` 为空）。这种由 shell 代发的 intent 无法真正把
  MediaProvider 的读权限授予第三方应用，所以还不能据此判定应用有问题；**需要用图库真实分享一次来验证**。
  收件箱放在 `Android/data/com.filedrop.app/files/` 正是为了这一步可以直接用 `adb shell ls` 看到结果。

## 这一轮的环境结论（会反复影响判断）

- `adb logcat -d` 完全空、bugreport 的 SYSTEM LOG 段也为空：HONOR 零售 ROM 限制，崩溃只能靠
  `dumpsys activity exit-info` + `dumpsys dropbox --print`（两者都读得到）。
- `run-as` 不可用（release 包），应用私有目录读不到 → 需要观测的中间产物统一放到
  `Android/data/<pkg>/files/` 这类外部私有目录。
- 服务只绑 Wi-Fi 地址，`adb forward tcp:18787 tcp:8787` 打不通；验证要走局域网
  `http://192.168.0.37:8787`，且 curl 必须 `--noproxy '*'`（本机代理会把局域网请求带走）。
- curl 的 `-F file=@/c/temp/...` 在 Windows 上要给 `C:/temp/...`，否则报 26（本地读不到文件），
  看起来像服务器问题。
- 这台手机在测试期间被其他应用/会话反复抢占前台（ideanote 的悬浮把手、HORIZON 录音框、相册），
  点击类验证必须先确认 `topResumedActivity` 是本应用再下结论；曾因此误判过一次「输入框无法输入」。

---

# 2026-11-07 13:20–14:0x 第三批反馈（接收端不完成 / 点文件报错 / 一直离线）

## 一、「发送端完成、接收端一直不完成」——文件其实到了，卡片是假的

先量事实：打开 `/storage/emulated/0/Download/FileDrop/` 看到

```
probe-1328.txt                     29 B   13:27   ← 我用 curl 发的探针
unknown                            34 B   13:12   ← 同一次探针（中文文件名那次）
屏幕截图 2026-08-27 132639.png   25996 B   13:12   ← 电脑发给手机的截图
```

PC 端 `~/Downloads/FileDrop/` 同时有 `s5.png`（13:13, 1.27 MB）和 `屏幕录制-20261107-131113.mp4`
（13:11, 1.07 MB）。也就是说两个方向的文件**都真的到了**，卡在 93% 的是界面上的进度条。

根因：`App.tsx` 里 `progress[message.id]` 只增不减。发送/接收结束、消息写进会话记录之后，
`syncPeer()` 会把 pending 清掉，但没有清 progress，于是这张已经完成的卡片一直显示最后一次进度
（93%）。修法：`syncPeer` 读到会话记录时，把已落库消息对应的 progress 条目一并删掉。

## 二、点会话里的文件

- 电脑发来的文件打不开：`FileProvider` 的 roots 没覆盖接收目录（见上一节），已加 `<root-path>`。
- 手机自己发出的文件点开提示「文件已经不在这个位置了」：发出的文件是从暂存副本上传的，传完会释放副本，
  卡片上的路径自然失效。这是设计使然，但文案误导，已改成
  「本机已经没有这个文件了：发出的文件在传输完成后会清理本地副本」。
  **产品取舍**：要「随时能打开我发出去的文件」就得长期保留副本，代价是应用私有目录随发送量增长；
  当前选择是只保留到对方收完（7 天上限），这两条路只能选一条，先把文案改对。

## 三、一直提示「对方离线」且发不出去

`outbox::reachable()` 的判据很直接：对端 id 是否在当前发现列表里。所以「在线却提示离线」= 发现列表里
短暂没有它，或它带着一个连不上的地址。证据链：

- 截图里出现过 `在线 · fe80::b646:f104:fc06:f3c3:8787`——地址是 **IPv6 链路本地**，没有 zone id 连不通。
- mDNS 的应答可能只带 AAAA（缓存/分片应答很正常），原实现会用它覆盖掉之前可用的 IPv4 host；
  下一次应答带上 IPv4 又恢复正常——这正是「一会离线一会在线」的形状。

修法（`discovery.rs`）：同网段 IPv4 → 任意 IPv4 → 非链路本地 IPv6 → 都没有则**保留原 host**，
最后才退到「有地址但可能连不上」——任何情况下都不把对端从列表里删掉（删掉就会被发送侧判成离线）。
配合发送侧的离线队列，即便某一刻确实连不上，消息也会在对方重新出现后补发。

## 四、MediaStore 仍然没有文件行（已知缺口，不是丢文件）

改成 `VOLUME_EXTERNAL_PRIMARY` 之后又发了一次探针：文件照样落在 `Download/FileDrop`（29 B，名字正确），
但 `content query --uri content://media/external/downloads --where "relative_path='Download/FileDrop/'"` 仍查不到
文件行——说明这台机器上「登记—建文件」这条路仍然没走通，实际写盘是 `claimOnDisk` 的直接路径写入
（在 Android 15 + 该 ROM 上可行）。**结论：文件不会丢、位置正确；但系统「下载」应用/相册可能要在媒体扫描后
才看到它。** 我先前把原因归给卷名的推断没有坐实（shell 插入成功、应用插入仍无行），这条留作已知缺口，
不在本轮继续猜。

要继续定位需要能读 `lastError()` 的调试包（release 包 `logcat` 与 `run-as` 都不可用），
这属于下一步工作，不影响当前「文件落在所选目录」的结果。

## 五、真凶：对外地址取成了蜂窝网卡（10.61.94.224）

装完后打开接收页，**「本机地址」显示 `10.61.94.224`**，而二维码地址仍是 `http://192.168.0.37:8787`。
这台手机同时开着 Wi‑Fi 和移动数据，`primary_interface()` 用的是「连一个公网 UDP 看默认路由」的办法——
Wi‑Fi 没有外网时，默认路由落在移动数据上，于是它拿到的是 `rmnet` 的 10.x 地址。

这个地址被用在两处，正好对上用户看到的现象：

1. **对外通告**：`FileDiscovery::start` 用 `primary_interface()` 作为 mDNS 的 A 记录注册；
   电脑解析到 10.61.94.224，连不上 → **电脑端一直显示手机离线、发不过去**。
2. **本机浏览**：mDNS 浏览走的是系统默认多播网卡；默认路由在蜂窝上时多播也可能从蜂窝出去 →
   **手机看不到电脑 → 发送时判「对方离线」**，与手机界面上「在线」自相矛盾。

修法（`net.rs`）：Android 上先找 `wlan*` / `ap*` / `eth*` 的 IPv4，跳过
`rmnet*`、`ccmni*`、`pdp*`、`clat*`、`tun*`、`dummy*`、`ip6tnl*`；找不到再退回原来的默认路由判断。
这一处同时修好「本机地址」「二维码地址」「mDNS 通告」「对端是否可达」四个显示/行为，桌面端逻辑不变。

**已验证**：装 13:59 那版后重新打开接收页，「本机地址」从 `10.61.94.224` 变成 **192.168.0.37**，
与二维码地址一致（截图）。电脑端要装同一批代码才会在它那一侧生效，所以 Windows 包也重打了。

另外，`outbox::reachable()` 对手动添加的会话（id 以 `manual:` 开头）恒为「离线」——它永远不会出现在
发现列表里，于是每次发送都排队并提示离线。已改为：手动会话按它自己记录的地址视为可达（地址非空即可），
发现型的会话仍按「是否在发现列表」判断，编辑器不额外等待（原有测试
`the_composer_does_not_wait_for_a_peer_discovery_has_not_seen` 保持通过，`cargo test --lib` 31 passed）。

## 六、这一轮的最终产物

| 产物 | 时间 | 体积 | 状态 |
| --- | --- | --- | --- |
| `app-arm64-release.apk` | 13:59 | 9.57 MB | 已装到荣耀X50（覆盖安装，配置与聊天记录保留） |
| `FileDrop_1.0.0_x64-setup.exe` | 15:44 | 2.11 MB | 已用 NSIS `/S` 静默覆盖安装到 `AppData\Local\FileDrop` |

装完后两端服务器都在：

```
GET http://192.168.0.27:8787/health → {"app":"FileDrop","name":"X-BOOKS","port":8787}
GET http://192.168.0.37:8787/health → {"app":"FileDrop","name":"荣耀X50","port":8787}
```

手机端 15:16 复核：接收页「本机地址 192.168.0.37」（与二维码一致）、设备名 荣耀X50、
保存位置 `Download/…`、`POST /upload` 落盘成功、点「我的」进程不退出。

## 七、ADB 已断开（15:2x 起）

手机被带离/关掉了无线调试，`adb devices` 为空，`adb connect 192.168.0.37:37895` 被拒绝。
所以下面三条仍是「代码与包已就位、待真机点一次」的状态：

1. 点会话里的文件卡片能否打开（`<root-path>` 修复）。
2. 图库分享里 FileDrop 的入口与发送面板（需要真实图库分享一次）。
3. 地址修正后的双向发送是否还有「对方离线」提示。

## 八、这轮学到的两条环境经验（避免下次误判）

- 手机同时开 Wi‑Fi 与移动数据时，**不能用「默认路由」代表局域网地址**；要看网卡类别。
  症状极具迷惑性：界面显示「在线」、服务器端口也在听，但对外通告的却是蜂窝 IP。
- 测试手机上跑着别的项目时（悬浮窗、录音、打车软件），`input tap` 会打到别的应用上。
  每次点击类检查前必须先读 `topResumedActivity`，否则会把「别的应用挡住」误判成应用缺陷。

---

# 2026-11-07 16:00–17:00 第四批反馈（手机开文件 / 终端闪现 / 通知点击 / 后台可达）

## 一、手机端会话里点文件仍报「无法打开文件」

上一版把崩溃换成了提示，但提示是守卫的统一文案（`无法打开文件`），把真正的 Java 异常盖掉了——
所以既分不清是 FileProvider 找不到 root，还是根本没有应用能打开这种文件。

做法：把「打开文件」整段搬进 Kotlin `StorageBridge.openFile(context, path)`：

- FileProvider 用 androidx 的那个类直接调用（在应用自己的 class loader 上，不再经 JNI 按名字找类）；
- MIME 表也在 Kotlin（`mimeOf`），不再从 Rust 传；
- 每一种失败都返回具体原因：拿不到 content URI（含异常类型与 message）、没有能打开的应用（附 MIME）、
  其它异常原文；成功返回 null。
- Rust 侧只负责把这段文字原样交给界面。

这条要等手机重新连上 ADB 后点一次才能定性：新的报错会直接指出失败在哪一步（provider 还是 viewer）。

## 二、PC 打开文件先弹一个终端

`cmd /C start "" <path>` 需要一个控制台，控制台会随 `cmd` 存在一小会儿再消失。已在 Windows 的
spawn 上加 `CREATE_NO_WINDOW (0x08000000)`，文件由系统的默认程序直接打开，不再有窗口闪一下。

## 三、PC 点击通知没有反应

根因：`tauri-plugin-notification` 在 Windows 上用 notify-rust 显示 toast，但**从不等待激活事件**，
点击之后没有任何代码被调用，窗口当然不会回来。notify-rust 本身提供 `wait_for_response`。

做法（仅 Windows）：

- 新增 `notify_win::show`：在工作线程里显示 toast，并在同一线程 `wait_for_response`；
  收到 `Default`（点击正文）时回到 UI 线程：`unminimize → show → set_focus`，并发出
  `notification-open { peerId }`；页面监听该事件，直接落到对应会话（不是只把窗口叫到前面）。
- 启动时调用 `SetCurrentProcessExplicitAppUserModelID("com.filedrop.app")`：安装器的快捷方式带了这个
  AUMID，所以「从快捷方式启动」时 toast 能显示；直接跑 exe（自启动、调试）时进程 AUMID 为空，
  toast 会被系统直接丢掉。显式声明后两种启动方式一致。
- **本机没能复现点击**：这台机器上 toast 会立刻 `Closed(Expired)`（约 20ms 内），
  用插件原来的同一条 notify-rust 调用也一样，说明是系统层面的抑制（通知设置/专注助手/注册状态），
  不是我们这条代码路径。代码用的是插件自带的同一个 `wait_for_response` API，装好后点一次即可确认。

## 四、手机在后台时，PC 看到在线但发送提示离线

CPU 的 partial wake lock 只保证进程不睡，**Wi-Fi 射频仍会自己进入省电**：对端在这个窗口里发起的连接
会超时，而 mDNS 记录还在，于是「显示在线、发不过去」。这类现象在锁屏/后台几分钟后最明显。

做法：`ServerService` 在持有组播锁、CPU 锁的同时再持有一把 `WifiLock`
（`WIFI_MODE_FULL_HIGH_PERF`），停止接收时一起释放。配合上一批修好的「对外通告 Wi-Fi 地址」，
后台可达性才完整。

## 五、验证进度

| 项 | 代码 | 本机证据 | 真机 |
| --- | --- | --- | --- |
| 手机打开文件 | 已改 Kotlin 内建 + 具体错误 | 编译通过（Kotlin/Rust 双端） | 待手机连回 ADB 点一次 |
| 终端闪现 | 已加 CREATE_NO_WINDOW | 编译通过 | 装后点文件卡片目视（闪现约 100ms） |
| 通知点击 | 已接管 toast 激活 | 已确认 toast 在本机被系统抑制（同链路），点击无法复现 | 装后点一次 toast |
| 后台可达 | 已加 WifiLock | — | 锁屏/后台几分钟后从 PC 发一条 |

Android 包与 Windows 包都会在本轮重新打并覆盖安装；ADB 目前仍不可用（手机不在同一网络或关闭了无线调试）。

### 包内证据（不看退出码，看 dex）

`app-arm64-release.apk`（16:55）解包后 `dexdump`：

```
StorageBridge.openFile   (Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;  PUBLIC STATIC FINAL
StorageBridge.lastError  ()Ljava/lang/String;                                            PUBLIC STATIC FINAL
StorageBridge.shareInbox (Landroid/content/Context;)Ljava/lang/String;                   PUBLIC STATIC FINAL
classes.dex 含 "filedrop:wifi"（WifiLock 的 tag）
```

说明 R8 没有把新方法裁掉，WifiLock 那一段也确实进了包——但**运行期**行为（点文件能否打开、
后台是否还可达）仍要真机验证。

Windows 包（16:47）已用 NSIS `/S` 覆盖安装，服务器已在 8787 上响应。

## 六、真机复测（USB/无线 ADB 恢复后，17:0x–17:4x）

### 1）手机点文件：通过

会话里点 PC 发来的 `哪吒入心之途.mp4`（77.8 MB）：

```
topResumedActivity=com.hihonor.hnvideoplayer/com.honor.videoplayer.MainActivity
```

系统的荣耀视频播放器被拉起，FileDrop 退到后台——不再是「无法打开文件」。
（这条同时也证明 `StorageBridge.openFile` + androidx FileProvider + `FLAG_GRANT_READ_URI_PERMISSION`
在共享存储路径上是对的。）

### 2）后台可达性：找到真机制，但 A/B 不完美，按事实记

先把症状量清楚。**锁屏 60 秒、未加白名单**时（无线 ADB）：

```
ping 192.168.0.37        → 0% 丢包（IP 层活着）
curl :8787/health        → 连接超时（http=000）
dumpsys wifi             → WifiLock{filedrop:wifi type=4} 持有
dumpsys power            → filedrop:server (partial) 持有
dumpsys activity services→ isForeground=true types=0x40000000（specialUse）
/proc/<pid>/stat         → state=D（不可中断睡眠）
```

也就是说：网络与端口都在，锁也都持有，但**应用进程当时处于 D 状态、不接连接**；
随后无线 ADB 也断了。仅凭这一次测量无法在「Doze 断网」「OEM 冻结」「进程卡在系统调用」
三者之间唯一定性，所以这一轮做的是把用户可用的那个开关补上并逐一测量：

| 状态 | 设备电源 | 结果 |
| --- | --- | --- |
| 强制 deep idle（`force-idle`，已加白名单） | USB | `health=200`，`/upload=200`「成功保存 1 个文件」 |
| 强制 deep idle（移除白名单，同一状态内） | USB | `health=200`（移除后系统不会回溯已运行的进程，只有下一次 idle 迁移才生效，所以这一格不能当反证） |
| 自然 dozing（`dumpsys battery unplug` + 息屏 100 秒，已加白名单） | 模拟电池 | `mWakefulness=Dozing`，`health=200` |
| 自然 dozing（同上，移除白名单 20 秒） | 模拟电池 | `health=200`（同上，迁移未发生） |
| 自然 dozing（未加白名单，用户现场那次） | 电池 | `health=000`（超时） |

结论按证据说：**加白名单后，在强制 deep idle 与自然 dozing 下接收都在线**；
而最初失败那次没有白名单。「移除白名单后立刻复现」没做成，因为 Android 只在 idle 迁移时应用策略，
我不会把它写成「已证明因果」，只写成：白名单是目前可用的、用户可授的开关，已在应用内提供。

### 3）应用内的入口

「我的 → 息屏后保持在线」：未允许时显示「去允许」，点了直接弹系统
`ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`（拿不到就退到电池优化列表）；已允许时显示「已允许」，
每次回到前台重读状态。清单已加 `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`。

### 4）仍未在本机验证的两项

- **Windows 通知点击**：本机 toast 被系统抑制（约 20ms 即 `Closed(Expired)`，插件同链路也一样），
  无法点击复现；代码用的是 notify-rust 官方 `wait_for_response`。
- **打开文件时不再闪终端**：`CREATE_NO_WINDOW` 已加，但 100ms 级的窗口闪现在截图里抓不稳，
  装好后点一次文件卡片目视即可。

---

# 2026-11-07 17:5x 「发送失败」改为自动重试（含文件夹能力结论）

## 一、这张卡为什么是「发送失败」

`chat_send_files` 的上传是一次性 TCP：`client::send_file` 一失败就 `mark_failed` 并 `return Err`——
**不重试、不保留已传字节**，卡片永久停在「发送失败」，错误串只写进界面提示，没进日志。
所以对方休眠、Wi‑Fi 抖一下、服务器重启，都会把 90% 的进度作废；这也是 17:40 那张卡片的机制成因
（具体错误码当时没留痕，拿不到）。

## 二、改法（已实现）

- 失败分支不再 `mark_failed`，而是走与「对方离线」完全相同的队列：`chats.enqueue(queued=true)`，
  outbox 每 5 秒重试一次；暂存副本在送达前不删。
- 提示语改为「N 个文件已排队，会自动重试」；错误原文写入 `tracing::warn`，下次可查。
- 只有不可恢复的情况（本地文件消失）才走 `drop_queued` → 卡片才显示失败。
- 删除不再使用的 `Chats::mark_failed`。
- 未做**断点续传**：需要在 `/upload` 上加 `Content-Range` 与两端协议配合，属于下一版工作量；
  当前版本会在失败后整份重传。

## 三、文件夹

**当前不支持**：选择器只给文件，协议里也没有相对路径字段。两条路：

- **A（推荐）**：发送时把文件夹打包成 `.zip` 再走现有协议。一端改动、协议不变、一张卡片；
  代价是临时空间 + 打包时间，大文件夹慢且不支持断点。
- **B**：协议加相对路径，逐文件上传，保留目录结构、可续传。两端都要改，明显更大。

未实现，等用户选定方案。

## 四、这一改的验证状态（如实）

- `cargo test --lib` 31 passed、host/Android `cargo check` 0 告警、`tsc --noEmit` 通过；
- 两端重新打包并安装：Windows 17:50、Android 17:59（`lastUpdateTime` 17:59）；
- **运行期 A/B 没做成**：手机开了应用锁，点「+」打开文件选择器时跳到
  `com.hihonor.systemmanager/...AuthLaunchLockedAppActivity`（需要机主指纹/密码），
  无法完成一次真实发送；用 shell 发起的 `ACTION_SEND` 又因授权限制进不了收件箱（收件箱为空）。
  所以「失败 → 排队 → 对方回来 → 自动送达」这条端到端，目前只有与离线队列共享的代码路径
  （已有单测）作证据，需要机主解锁后真实发一次来确认。

---

# 2026-11-07 18:0x 同一条消息重复出现：按消息 id 做幂等

## 一、现象与根因

接收端同一条「vvcnn」出现两次（截图 17:52 两条 17:52）。根因不在显示层：

- 接收端 `insert()` **无条件** `messages.push()`，没有任何去重键；
- 发送端在对端**已经收到但回执丢失**时会重发——`send_now` 没拿到确认就把条目留在 outbox，
  tick 下个 5 秒再发一次；
- 于是同一条消息被记录两次。文件同理：重发会再写一份字节，落成 `xxx (1).mp4`
  （17:41 那张 `哪吒入心之途 (1).mp4` 就是这种「已经到过又重发」的产物）。

## 二、改法（幂等键 = 发送端的消息 id）

- **文本**：`POST /message` 增加 `id` 字段；接收端用它当消息 id 入库。
- **文件**：multipart 增加 `messageId` 字段（在文件字段之前）；接收端用它做消息 id 与进度事件 id，
  保证卡片和进度同一把钥匙。
- **去重**：`insert()` 先按 id 查，已存在就直接返回已有消息（不新增、不重复计未读、不覆盖预览）；
  文件路径在**写字节之前**先查，已存在就把这一段读掉并回 200，不落第二份文件。
- 新增单测 `a_resent_message_is_not_a_second_bubble`；`cargo test --lib` **32 passed**。

## 三、边界与未验证

- 旧版本已经写重的气泡不会被自动清理（没有可靠依据判断哪条是重复），需要的话得手工删。
- 重发场景的端到端验证需要真机：发送端在对端收到后、回执丢失时重发，接收端应当只保留一条。
  两端重打包后各发一条即可观察；本轮只做了单测与编译验证。

### 接收端已实测（电脑，18:13）

用同一个 `id` 连发两次 `POST /message`（正文 `dedupe test one`，`fromId=peer-dedupe`）：

```
first  http=200 {"ok":true}
second http=200 {"ok":true}
```

会话里只有**一条** `dedupe test one` 气泡（截图经 Computer Use 抓取），列表预览也只有一条。
说明「同一 id 重发不产生第二条」在 Windows 端成立。

（第一次用 shell 内联中文 JSON 时两次都是 400，是 shell 引号把 JSON 拼坏了，属于测试写法问题，
改用文件负载后即 200，不是服务端问题。）
