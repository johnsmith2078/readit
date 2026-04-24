# Tauri 2 桌面划词/悬停朗读应用可行性分析与实施计划

生成日期：2026-04-24

## 1. 结论摘要

这个想法**可行，但不应承诺“任何界面都能稳定识别段落”**。更现实的产品定义是：

- 在主流浏览器、PDF 阅读器、Office/文本编辑器等支持系统无障碍接口的应用中，优先通过系统 Accessibility / UI Automation API 获取鼠标下方文本。
- 对无法暴露文本语义的界面，例如截图式 PDF、Canvas/WebGL、游戏、远程桌面、部分 Electron/Qt/自绘控件，降级为 OCR 或手动选中文本朗读。
- TTS 使用 `edge-tts` 可快速实现高质量朗读，但它依赖微软 Edge 在线语音服务，适合作为 MVP；若要长期商用，需要评估服务稳定性、许可与替代 TTS 后端。

推荐 MVP：**Windows 优先**。Windows 的 UI Automation 对跨应用文本识别支持最好，Tauri 2 也适合做轻量桌面壳、设置页、托盘、快捷键和进程编排。

## 2. 目标功能拆解

用户目标：鼠标悬停在文本段落上，点击即可朗读。

建议拆成四个子系统：

1. **鼠标与交互监听**
   - 监听全局鼠标位置、点击、快捷键。
   - 判断当前鼠标下方 UI 元素。
   - 提供开关：悬停后显示小浮窗，点击浮窗或快捷键朗读，避免拦截普通点击。

2. **文本识别与段落提取**
   - 优先通过平台无障碍 API 获取鼠标点下的文本范围。
   - 将句子/行扩展为段落。
   - 对不可访问文本降级为 OCR。

3. **TTS 播放管线**
   - 调用 `edge-tts` 生成音频流或临时音频文件。
   - 使用 Rust/前端音频播放器播放、暂停、停止、调速、切换声音。
   - 做缓存，避免重复请求同一段文本。

4. **Tauri 2 应用层**
   - 设置界面、权限引导、托盘菜单、状态提示。
   - Tauri command 连接前端和 Rust 后端。
   - 管理 Python sidecar 或改用 Rust TTS 适配层。

## 3. 关键技术可行性

### 3.1 Tauri 2 作为桌面框架

Tauri 2 适合这个项目：

- Rust 后端适合调用 Windows UI Automation、macOS Accessibility、Linux AT-SPI 等系统 API。
- 前端可以用 React/Vue/Svelte 等实现设置页与浮窗 UI。
- Tauri 2 的权限/能力模型更严格，需要显式配置 shell、global-shortcut 等插件权限。
- 可以通过 sidecar 方式打包和调用外部二进制，例如 Python 打包后的 `edge-tts` 辅助进程。

注意点：

- Tauri 的 WebView 本身不负责跨应用读取文本，真正难点在 Rust 后端的系统 API。
- 跨平台差异非常大，不建议一开始同时做 Windows/macOS/Linux。

### 3.2 跨应用文本识别

#### Windows：最推荐首发

技术路径：Windows UI Automation。

可行能力：

- 根据鼠标屏幕坐标找到 UI Automation 元素。
- 对支持 TextPattern/TextPattern2 的控件，可通过文本范围获取字符、单词、行、段落附近内容。
- 浏览器、Edge/Chrome、部分 PDF 阅读器、Office、记事本等通常会暴露一定文本信息。

限制：

- 不是所有应用都暴露文本。
- PDF 的可访问性质量取决于 PDF 文件和阅读器。
- 网页中 Canvas、图片文字、远程桌面无法直接读取。
- 悬停即识别可能频繁触发，需要节流与缓存。

建议方案：

- 使用 Rust `windows` crate 直接调用 UI Automation COM API，或评估成熟封装库。
- MVP 先实现：鼠标点击时读取当前位置文本，而不是持续悬停扫描。
- 先支持“单词/行/段落附近文本”，再优化段落边界。

#### macOS：可行但权限和兼容成本更高

技术路径：Accessibility API / AXUIElement。

可行能力：

- 通过辅助功能权限访问其他应用 UI 元素。
- 对文本控件可读取 selected text、value、range、bounds 等属性。

限制：

- 必须引导用户授予辅助功能权限。
- 不同应用暴露的 AX 属性差异很大。
- 鼠标点精确映射到文本 range 的实现成本较高。

建议：

- 第二阶段支持。
- 先做“选中文本后快捷键朗读”，再做鼠标点下文本范围。

#### Linux：可行但桌面环境差异大

技术路径：AT-SPI / ATK。

可行能力：

- GNOME/KDE 下很多 GTK/Qt 应用可通过 AT-SPI 暴露文本。
- 可读取 Text interface 中的字符、单词、句子、行等。

限制：

- Wayland 安全模型限制全局监听和屏幕坐标能力。
- 不同发行版、桌面环境和应用支持差异明显。
- 打包和权限引导复杂。

建议：

- 第三阶段支持。
- 优先 X11/GNOME 环境，Wayland 做明确限制说明。

### 3.3 OCR 降级方案

OCR 是“任何界面”的必要补充，但不建议作为第一优先路径。

可选方案：

- Windows OCR API：系统集成度高，但调用和语言包处理需要额外开发。
- Tesseract：离线、跨平台，但中文效果和部署体积需要调优。
- PaddleOCR/ONNX：中文效果更好，但模型体积和集成复杂度更高。

建议 MVP 降级策略：

- 第一版不做 OCR，只提示“当前应用未暴露可朗读文本”。
- 第二版增加“按快捷键截取鼠标附近区域 OCR 后朗读”。
- OCR 只作为显式触发，不做持续悬停 OCR，避免性能和隐私问题。

### 3.4 edge-tts 集成

`edge-tts` 是 Python 包，可调用微软 Edge 在线 TTS 服务生成语音。

集成方式选项：

1. **sidecar CLI**
   - 用 PyInstaller 将 Python 脚本打包为可执行文件。
   - Tauri/Rust 通过 shell sidecar 调用。
   - 优点：实现快，和现有 `edge-tts` 生态兼容。
   - 缺点：包体更大，进程启动有延迟。

2. **长期运行的本地 TTS helper**
   - sidecar 启动本地子进程，通过 stdin/stdout 或 localhost IPC 通信。
   - 优点：延迟更低，可维护队列和缓存。
   - 缺点：进程生命周期管理复杂。

3. **Rust 直接实现协议或使用非官方库**
   - 优点：包体和调用链更简单。
   - 缺点：协议变动风险更高，维护成本更高。

MVP 推荐：**Python sidecar + 临时 mp3 文件 + Rust 播放**。

需要关注：

- `edge-tts` 依赖网络。
- 需要处理限流、失败重试、超时、代理。
- 长文本要分段合成，避免请求过长。
- 商用前必须审查 `edge-tts` 和微软服务条款风险。

## 4. 产品风险与边界

### 4.1 技术风险

- “任何界面”无法完全保证，系统安全模型和应用实现会阻止读取。
- 鼠标下文本到段落边界的映射不总是可靠。
- PDF 支持高度依赖阅读器和 PDF 本身是否有文本层。
- OCR 会引入隐私、性能、准确率和模型体积问题。
- TTS 在线服务不稳定或协议变化会影响可用性。

### 4.2 权限与隐私风险

该应用会读取其他应用界面的文本，必须明确告知用户：

- 读取范围：仅在用户触发悬停/点击/快捷键时读取鼠标附近文本。
- 上传范围：使用 `edge-tts` 时，待朗读文本会发送到在线 TTS 服务。
- 本地缓存：音频缓存、文本缓存是否保存，保存多久，如何清除。
- 敏感场景：密码框、支付页面、隐私窗口、特定应用黑名单。

建议默认：

- 不读取密码框。
- 不自动上传未确认文本。
- 支持应用黑名单。
- 支持关闭缓存或一键清理缓存。

## 5. 推荐 MVP 范围

平台：Windows 10/11。

MVP 功能：

- Tauri 2 应用框架。
- 系统托盘 + 设置页。
- 全局快捷键，例如 `Ctrl+Alt+R`：读取鼠标当前位置文本并朗读。
- 鼠标点击触发模式可选，但默认不用“直接拦截点击”。
- UI Automation 获取文本。
- `edge-tts` sidecar 合成 mp3。
- 播放、暂停、停止。
- 声音、语速、音量配置。
- 错误提示：未识别文本、无网络、未找到 TTS helper。

不建议 MVP 做：

- 真正意义上的全平台支持。
- 持续 OCR。
- 自动读取所有鼠标悬停文本。
- 对每个 PDF 阅读器做深度适配。
- 完整生词本/翻译/笔记系统。

## 6. 技术架构草案

```text
┌────────────────────────────┐
│ Tauri 2 Frontend           │
│ Settings / Overlay / Tray  │
└──────────────┬─────────────┘
               │ invoke/events
┌──────────────▼─────────────┐
│ Rust Backend               │
│ - global shortcut          │
│ - mouse position           │
│ - accessibility adapter    │
│ - tts manager              │
│ - audio playback           │
└───────┬───────────┬────────┘
        │           │
        │           ▼
        │    edge-tts sidecar
        │    text -> mp3
        │
        ▼
Platform text adapters
- Windows UI Automation
- macOS Accessibility
- Linux AT-SPI
```

Rust 模块建议：

```text
src-tauri/src/
  main.rs
  app_state.rs
  text_capture/
    mod.rs
    windows_uia.rs
    macos_ax.rs
    linux_atspi.rs
  tts/
    mod.rs
    edge_tts_sidecar.rs
    cache.rs
  audio/
    mod.rs
  privacy/
    filters.rs
```

前端模块建议：

```text
src/
  App.tsx
  pages/Settings.tsx
  components/FloatingToolbar.tsx
  components/StatusToast.tsx
  lib/tauri.ts
```

## 7. 分阶段实施计划

### 阶段 0：技术验证，1-2 天

目标：证明 Windows 上能读取鼠标下文本并朗读。

任务：

- 创建 Tauri 2 项目。
- 写 Rust 原型：获取鼠标坐标，调用 UI Automation 获取元素和文本。
- 测试目标：Chrome 网页、Edge PDF、Adobe Reader 或 SumatraPDF、记事本、VS Code。
- 写 Python `edge-tts` helper：输入文本，输出 mp3。
- Rust 调用 helper 并播放 mp3。

验收：

- 在 Chrome 普通网页中，鼠标指向文本后按快捷键能朗读附近一句或一段。
- 不支持的控件能返回明确错误。

### 阶段 1：MVP，1-2 周

目标：形成可日常试用的 Windows 版本。

任务：

- 完成 Tauri 设置页。
- 配置全局快捷键。
- 实现 UI Automation 文本捕获适配器。
- 实现段落边界推断：优先段落，其次行/句子。
- 集成 `edge-tts` sidecar。
- 实现播放控制和状态提示。
- 增加文本长度限制、分段合成、缓存。
- 增加隐私设置：黑名单、缓存清理、上传提示。

验收：

- 浏览器文章、普通 PDF 文本层、记事本文本可朗读。
- 网络断开、TTS 失败、无法读取文本时提示清晰。
- 应用可打包安装。

### 阶段 2：体验增强，2-4 周

目标：提升识别率和使用体验。

任务：

- 增加鼠标悬停后浮动按钮，而不是直接点击文本就朗读。
- 增加手动选中文本朗读。
- 增加 OCR 降级：快捷键截取鼠标附近区域识别。
- 增加更多 TTS 配置：voice、rate、volume、pitch。
- 增加音频队列和预加载。
- 增加最近朗读历史，可选择是否保存。

验收：

- 对图片型 PDF 或截图网页，可通过 OCR 快捷键朗读。
- 悬停浮窗不影响正常鼠标点击。

### 阶段 3：跨平台，4-8 周以上

目标：扩展 macOS/Linux，但接受能力差异。

任务：

- macOS Accessibility 适配器。
- macOS 权限引导和检测。
- Linux AT-SPI 适配器。
- Wayland/X11 能力检测。
- 平台差异文档和兼容性矩阵。

验收：

- macOS 支持选中文本朗读和部分文本控件鼠标定位。
- Linux 在 GNOME/X11 或明确环境中可工作。

## 8. 关键实现建议

### 8.1 交互方式

不建议默认“鼠标浮动在文本段上，直接点击文本就朗读”。原因：

- 会和网页链接、PDF 选择、编辑器点击等正常行为冲突。
- 全局鼠标 hook 容易被安全软件拦截或让用户不信任。

推荐交互：

1. 鼠标停留 300-600ms 后，如果识别到文本，显示一个很小的朗读按钮。
2. 用户点击朗读按钮或按快捷键触发。
3. 支持“按住 Alt 点击文本朗读”的高级模式。

### 8.2 段落识别策略

优先级：

1. UI Automation / Accessibility 直接提供段落范围。
2. 通过文本 range 扩展到包含鼠标点的行，再向上下合并，直到空行或明显段落边界。
3. 对网页可考虑浏览器扩展辅助，但这会偏离“任何界面”的原始目标。
4. OCR 模式按版面分析得到文本块。

### 8.3 TTS 分段策略

- 清理文本：去掉重复空白、页眉页脚、过长 URL。
- 按段落和句号分块，每块控制在合理长度。
- 顺序合成并播放，或先合成第一块再后台合成后续块。
- 对相同文本和 voice/rate 参数生成缓存 key。

### 8.4 隐私策略

建议在设置页中明确展示：

- “朗读会将文本发送到 Microsoft Edge 在线语音服务”。
- “不支持读取密码框”。
- “可添加应用黑名单”。
- “可关闭本地历史和音频缓存”。

## 9. 建议技术栈

- 桌面框架：Tauri 2
- 前端：React + TypeScript 或 Svelte + TypeScript
- Rust 平台 API：`windows` crate / macOS Accessibility FFI / Linux AT-SPI bindings
- TTS：Python `edge-tts` sidecar
- Python 打包：PyInstaller
- 音频播放：Rust `rodio` 或前端 WebAudio/HTMLAudio
- OCR 后续：Windows OCR API 或 PaddleOCR/ONNX Runtime

## 10. 开发里程碑清单

- [ ] 初始化 Tauri 2 项目。
- [ ] Windows 获取鼠标坐标。
- [ ] Windows UI Automation 命中测试。
- [ ] 获取鼠标下文本 range。
- [ ] 段落边界推断。
- [ ] `edge-tts` helper 原型。
- [ ] Rust 调用 TTS helper。
- [ ] 音频播放控制。
- [ ] 全局快捷键触发朗读。
- [ ] 设置页：voice/rate/volume。
- [ ] 隐私提示和黑名单。
- [ ] 打包 sidecar。
- [ ] 安装包构建。
- [ ] Chrome/PDF/记事本兼容性测试。
- [ ] OCR 降级方案。

## 11. 资料来源

- Tauri 2 官方文档：插件、权限、sidecar、global shortcut 能力。
- Microsoft Windows UI Automation 文档：TextPattern、TextRange、基于屏幕坐标获取 UI 元素。
- Apple Accessibility API 文档：AXUIElement 与辅助功能权限。
- Linux AT-SPI 文档：Text interface 与可访问对象树。
- `edge-tts` PyPI/GitHub 文档：命令行与 Python API 生成语音。

## 12. 最终建议

建议按“Windows + 快捷键读取鼠标下文本 + edge-tts 朗读”启动。这样能最快验证核心价值，并避免一开始陷入跨平台、全局点击拦截和 OCR 的复杂度。

如果验证成功，再逐步加入悬停浮窗、OCR 降级和 macOS/Linux 适配。产品对外描述应从“任何界面”调整为“在大多数支持无障碍文本的应用中朗读鼠标附近文本，并提供 OCR 降级”。
