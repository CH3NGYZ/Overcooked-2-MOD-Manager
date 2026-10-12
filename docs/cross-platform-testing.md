# 跨平台测试指南

## 方案 1: 使用虚拟机测试 (推荐)

### Linux 测试
1. **下载 Ubuntu 22.04 Desktop ISO**
   - 官网: https://ubuntu.com/download/desktop

2. **使用 VirtualBox 或 VMware 创建虚拟机**
   - 分配至少 4GB 内存, 20GB 硬盘

3. **在虚拟机中测试:**
   ```bash
   # 安装依赖
   sudo apt update
   sudo apt install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf

   # 安装 Node.js 和 Rust
   curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -
   sudo apt install -y nodejs
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

   # 克隆代码并构建
   git clone <你的仓库>
   cd Overcooked-2-MOD-Manager-Code
   npm install
   npm run tauri build
   ```

4. **测试验证:**
   - 检查 Steam 路径检测是否正确
   - 查看 Linux 教程是否显示
   - 测试启动参数复制功能

### macOS 测试 (需要 macOS 环境)
1. **使用真实 Mac 或 Hackintosh**
   - 虚拟化 macOS 较为复杂,建议使用真机

2. **在 macOS 中测试:**
   ```bash
   # 安装 Xcode Command Line Tools
   xcode-select --install

   # 安装 Node.js 和 Rust
   brew install node
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

   # 克隆代码并构建
   git clone <你的仓库>
   cd Overcooked-2-MOD-Manager-Code
   npm install
   npm run tauri build
   ```

3. **测试验证:**
   - 启动管理器,检查是否自动显示 macOS 教程
   - 验证 17 步教程内容是否正确显示
   - 检查教程图片是否正确加载

---

## 方案 2: GitHub Actions 构建测试 (自动化)

推送代码到 GitHub 后,GitHub Actions 会自动在 3 个平台编译:

1. **触发编译:**
   ```bash
   git push origin main
   # 或创建 tag 触发 Release
   git tag v1.2.0
   git push origin v1.2.0
   ```

2. **查看构建日志:**
   - 访问 GitHub 仓库的 Actions 页面
   - 查看 3 个平台的编译日志
   - 下载编译产物测试

3. **验证点:**
   - ✅ Linux x64 编译成功
   - ✅ macOS ARM64 编译成功
   - ✅ macOS x64 编译成功
   - ✅ Windows x64 编译成功(对照组)

---

## 方案 3: 条件编译检查 (代码层面)

验证跨平台代码是否正确使用条件编译:

```bash
# 检查 Linux 目标编译
cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-unknown-linux-gnu

# 检查 macOS 目标编译
cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-apple-darwin
cargo check --manifest-path src-tauri/Cargo.toml --target aarch64-apple-darwin
```

**注意:** 需要先安装对应的 Rust target:
```bash
rustup target add x86_64-unknown-linux-gnu
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin
```

---

## 方案 4: 远程真机测试服务

使用云测试服务提供的真机环境:
- **BrowserStack** (支持远程 macOS)
- **AWS Device Farm**
- **MacStadium** (macOS 专用)

---

## 推荐测试顺序

1. ✅ **代码审查** - 检查逻辑是否正确(已完成)
2. 🔄 **GitHub Actions** - 推送代码,检查编译是否通过
3. 💻 **Linux 虚拟机** - 使用 VirtualBox 测试 Linux 功能
4. 🍎 **macOS 真机** - 找有 Mac 的朋友或使用云服务测试

---

## 当前状态

✅ 代码已实现并通过编译
✅ GitHub Actions 配置已完成
⏳ 等待推送到 GitHub 触发自动构建
⏳ 需要真机或虚拟机进行功能测试
