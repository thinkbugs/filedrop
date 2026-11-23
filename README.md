# FileDrop

局域网内点对点传文件：Windows / macOS / Android 之间直接互传，不经过任何服务器，数据不出本地网络。

界面按微信的对话模型组织——每台设备就是一个联系人，发文件就是发消息，收到的文件自动落进本机文件夹。

## 功能

- **零配置发现**：mDNS 自动发现同一局域网内的其他设备，不需要扫码、不需要配对
- **点对点传输**：每条消息都由接收方内嵌的 HTTP 服务直接接收，文件不上传云端
- **会话式界面**：文字 + 文件混合的聊天流，发送进度、失败重试、已读状态都在气泡里
- **离线排队**：对方不在线时消息进发件箱，每 5 秒探测一次，对方上线后自动补发
- **消息幂等**：每条消息带唯一 ID，网络抖动导致的重复投递只会显示一条
- **断点续传失败可恢复**：发送失败的文件保留暂存副本，重试从副本重新发送，不需要重新选文件
- **系统集成**
  - Windows：资源管理器右键「FileDrop 传送」、系统通知点击直达会话、接收端后台常驻
  - Android：前台服务常驻接收、电池优化白名单引导、相册/文件管理器「分享」到 FileDrop、收到文件写入系统「下载」目录

## 平台支持

| 平台 | 状态 |
| --- | --- |
| Windows | 已发布，安装包由 `tauri build` 生成 |
| Android (arm64) | 已发布，APK 由 `tauri android build` 生成 |
| macOS | 代码可编译，未做真机验证 |
| iOS | 未支持 |

## 技术栈

- **壳**：[Tauri 2](https://tauri.app/)（Rust + 系统 WebView），桌面与 Android 共用同一套 Rust 后端
- **前端**：React 18 + TypeScript + Vite 6，手写 CSS（无 UI 框架）
- **传输协议**：沿用 [Snapdrop](https://github.com/RobinLinus/snapdrop)（MIT）的思路——每台设备监听端口收发，`/health` 探活、`/text` 收文字、`/upload` 收文件
- **发现**：`mdns-sd` 组播广播设备名与端口；Android 上额外持有组播锁 / Wi-Fi 锁 / 前台服务，避免息屏掉线
- **存储**：Android 通过 MediaStore 认领「下载」目录下的落点，权限不足时自动回退到应用私有目录，保证文件不丢

## 开发

```bash
npm install
npm run tauri dev          # 桌面开发
npm run tauri build        # 桌面打包
```

Android 需要 JDK 17、Android SDK + NDK 27，并先执行一次 `npm run tauri android init`：

```bash
npm run tauri android build -- --target aarch64
```

Rust 侧测试：

```bash
cd src-tauri && cargo test --lib
```

## 目录结构

```
src/                     React 前端（会话、设备列表、设置、发送面板）
src-tauri/src/
  transfer/              传输核心
    server.rs            接收服务（/health /text /upload）
    client.rs            发送端请求
    discovery.rs         mDNS 发现与地址选择
    outbox.rs            离线队列与自动补发
    chat.rs              会话消息存储
    stage.rs             大文件分块暂存
    config.rs            设备名、端口、保存位置
  android.rs             JNI 桥接（打开文件/文件夹、前台服务、电池白名单）
  notify_win.rs          Windows 可点击通知
  commands.rs            Tauri 命令注册
src-tauri/gen/android/   Tauri 生成的 Android 工程（Kotlin 侧在 app/src/main/java）
```

## 已知限制

- 仅在同一局域网可用（依赖 mDNS 组播；跨网段、开启 AP 隔离的路由器下发现不到设备）
- 超大文件中断后需要整份重传（未实现 `Content-Range` 续传）
- 暂不支持发送文件夹（可先压缩后再发）
- Android 上收到 0 字节或特殊 MIME 的文件可能无法直接打开，需要到「下载」目录里手动查看

## 验证记录

真机与桌面端的测试明细（含每一步的设备、时间、证据）见 [TEST-REPORT.md](TEST-REPORT.md)。

## 许可

MIT，见 [LICENSE](LICENSE)。传输协议源自 Snapdrop（MIT）。