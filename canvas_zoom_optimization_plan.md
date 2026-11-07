# GPUI 无限画布缩放性能优化规划方案

> **文档定位**：针对 GPUI 视口缩放与画布复用场景，基于性能瓶颈实测数据（Dev 模式 30fps，其中 80% 耗时卡在 `inner(replay)` 的图元单项拷贝），规划的三种技术演进路线。重点详述**连续块切片批量拷贝（Contiguous Chunk Copy）**，并附**子场景 GPU 矩阵变换**与**离屏贴图缓存**路线。

---

## 1. 背景与瓶颈诊断

### 1.1 实测性能数据（已应用 `paint_layer` 后）
在 100 张 Markdown 卡片（约 1,600 个 `Quad`，12,600 个文字字形 `SubpixelSprite`，总计 14,200 个图元）在 100% 命中 View 缓存时的实测单帧开销：

```text
[PERF] with_scaled_paint: inner(replay)=29.0~32.6ms, scale_since=220µs, quads=1600, mono=0, subpx=12600
[PERF] Window::draw total=34.9~39.8ms (roots=33.4~38.1ms, finish=0.9~1.5ms)
[PERF] present(DirectX)=565~928µs
```

### 1.2 核心根因剖析
1. **GPUI 的 View 缓存本质是“CPU 指令流切片”，非“显存位图”**：
   - `.cached(...)` 仅跳过了组件树的 DOM 计算与 Taffy Layout 布局排版；
   - 每一帧绘制时，系统依然必须将上一帧记录的图元重新注入到当前帧的 `Scene` 容器中。
2. **逐项分发与逐项入队的遍历开销占满帧时间（30ms+）**：
   - 之前应用的 `paint_layer` 成功消除了 R-Tree 空间索引树计算（`BoundsTree::insert`）；
   - 但 [`Scene::replay`](file:///c:/Users/17904/Desktop/gpui_zoom_test/patches/gpui-pre/src/scene.rs#L141-L149) 内部仍是逐项迭代：
     ```rust
     for operation in &prev_scene.paint_operations[range] {
         match operation {
             PaintOperation::Primitive(primitive) => self.insert_primitive(primitive.clone()),
             ...
         }
     }
     ```
   - 在未开启编译器内联与 SIMD 优化的 Windows Dev 构建下，对 14,200 个图元执行逐个 `match` 分发、逐个 `clone()`、逐个动态扩容入队（`push`）与切片越界检查，产生了极高的指令冗余，占据了整帧耗时的 85%。

---

## 2. 核心路线：连续内存块切片批量拷贝（Contiguous Chunk Copy）

### 2.1 架构原理
在 `paint_layer` 内部，所有图元直接继承同一个 `DrawOrder`，在内存中具备以下关键特性：
1. **图元存储物理连续**：同一 Layer 内生成的 `Quad` 和 `SubpixelSprite` 在 `Scene` 的底层 `Vec` 中天然是连续存放的。
2. **无需逐个计算层级**：所有图元的 `order` 相同，复用时不需要重新跑图元级拦截与相交检测。
3. **可直接降维为内存切片拷贝（`memcpy`）**：
   将缓存索引从“操作记录流区间”升级为“各类图元底层物理数组的范围切片（`Range<usize>`）”，复用时直接调用 Rust 的 `extend_from_slice`。

```
【当前机制：逐项处理（~30ms）】
prev_scene.paint_operations[range]
  ├── [0] Clone -> insert_primitive -> match -> push
  ├── [1] Clone -> insert_primitive -> match -> push
  └── ... (循环 14,200 次)

【优化机制：切片批拷贝（~0.3ms）】
scene.quads.extend_from_slice(&prev_scene.quads[cached_quad_range]);              // 1 次底层 memcpy
scene.subpixel_sprites.extend_from_slice(&prev_scene.subpixel_sprites[cached_sprite_range]); // 1 次底层 memcpy
```

### 2.2 具体修改点规划

#### 修改点 1：扩展 `PaintIndex`，记录各图元数组物理范围
* **文件**：[`patches/gpui-pre/src/window.rs`](file:///c:/Users/17904/Desktop/gpui_zoom_test/patches/gpui-pre/src/window.rs)
* **改动**：在 `PaintIndex` 中记录当前帧 `scene.quads.len()`、`scene.subpixel_sprites.len()`、`scene.monochrome_sprites.len()` 等快照指针。
```rust
#[derive(Clone, Debug, Default)]
pub(crate) struct PaintIndex {
    pub scene_index: usize,
    pub quads_index: usize,
    pub subpixel_sprites_index: usize,
    pub monochrome_sprites_index: usize,
    pub paths_index: usize,
    pub shadows_index: usize,
    pub underlines_index: usize,
    // ... 其余现有字段（mouse_listeners_index 等）保持不变
}
```

#### 修改点 2：在 `Scene` 增加批量克隆方法 `replay_slices`
* **文件**：[`patches/gpui-pre/src/scene.rs`](file:///c:/Users/17904/Desktop/gpui_zoom_test/patches/gpui-pre/src/scene.rs)
* **改动**：绕过 `PaintOperation` 循环，直接对连续图元段执行 `extend_from_slice`。
```rust
impl Scene {
    pub fn replay_slices(
        &mut self,
        prev_scene: &Scene,
        quads_range: Range<usize>,
        subpixel_range: Range<usize>,
        mono_range: Range<usize>,
        shadows_range: Range<usize>,
        paths_range: Range<usize>,
        underlines_range: Range<usize>,
    ) {
        if !quads_range.is_empty() {
            self.quads.extend_from_slice(&prev_scene.quads[quads_range]);
        }
        if !subpixel_range.is_empty() {
            self.subpixel_sprites.extend_from_slice(&prev_scene.subpixel_sprites[subpixel_range]);
        }
        if !mono_range.is_empty() {
            self.monochrome_sprites.extend_from_slice(&prev_scene.monochrome_sprites[mono_range]);
        }
        if !shadows_range.is_empty() {
            self.shadows.extend_from_slice(&prev_scene.shadows[shadows_range]);
        }
        if !paths_range.is_empty() {
            self.paths.extend_from_slice(&prev_scene.paths[paths_range]);
        }
        if !underlines_range.is_empty() {
            self.underlines.extend_from_slice(&prev_scene.underlines[underlines_range]);
        }
    }
}
```

#### 修改点 3：在 `Window::reuse_paint` 中接入批量回放
* **文件**：[`patches/gpui-pre/src/window.rs`](file:///c:/Users/17904/Desktop/gpui_zoom_test/patches/gpui-pre/src/window.rs)
* **改动**：在复用 Paint 范围时，如果当前视图位于独立 `Layer` 内，直接调用 `scene.replay_slices(...)`，而不再走老旧的 `scene.replay(...)` 操作流循环。

### 2.3 性能预期与影响评估
* **实现复杂度**：低（预估工作量：0.5 ~ 1 人天，纯 Rust 逻辑，不涉及平台渲染器或 Shader）。
* **Dev 模式预期表现**：`inner(replay)` 耗时从 **30ms 骤降至 0.2~0.4ms**，Dev 单帧总耗时压至 **2~4ms**，直接突破 100fps 并触达屏幕物理刷新率。
* **兼容性与风险**：
  - 仅在 `paint_layer` 内部生效（此时层内图元无须 R-Tree 树排重，语义完全合法）；
  - 保留了原有的 Hitbox、鼠标监听器及焦点树复用机制，对业务代码零破坏。

---

## 3. 路线 2：独立子场景（Sub-Scene）+ GPU 顶点 Shader 矩阵变换

### 3.1 架构原理
彻底将变换从 CPU 计算中剥离，移交至 GPU 顶点着色器（Vertex Shader）：
1. **CPU 零拷贝**：为 `CardsCanvas` 分配一个持久化的 `SubScene`，在 Cache Hit 期间**完全不执行内存拷贝，也不清空**。
2. **GPU 顶点变换**：废弃 `Scene::scale_since` 的 CPU 浮点遍历修改，在 DirectX 11 的 ConstantBuffer 中引入 `CanvasTransform` 矩阵：
   $$\begin{pmatrix} X_{\text{screen}} \\ Y_{\text{screen}} \end{pmatrix} = \begin{pmatrix} \text{scale} & 0 \\ 0 & \text{scale} \end{pmatrix} \begin{pmatrix} X_{\text{local}} - X_{\text{pivot}} \\ Y_{\text{local}} - Y_{\text{pivot}} \end{pmatrix} + \begin{pmatrix} X_{\text{pivot}} + \text{pan}_x \\ Y_{\text{pivot}} + \text{pan}_y \end{pmatrix}$$
3. **多批次提交流水线**：
   - Pass 1：主场景背景绘制（使用标准屏幕正交投影矩阵）；
   - Pass 2：应用 `CanvasTransform` 矩阵，直接使用显存中原有的 SubScene 顶点缓冲区绘制卡片阵列；
   - Pass 3：切回标准正交矩阵，绘制 `FpsMonitor` 及浮动 UI。

### 3.2 关键改动点与难点
* **改动层级**：`patches/gpui-pre-windows/src/directx_renderer.rs` 及 `shaders.hlsl`。
* **难点 1（着色器改动）**：
  修改所有管线着色器（`quad`、`sprite`、`shadow`、`path`），支持接受动态模型变换矩阵（Model Matrix）。
* **难点 2（Scissor 裁剪转换）**：
  DirectX 11 的硬件剪裁（`RSSetScissorRects`）工作在屏幕设备像素空间，如果图元在顶点着色器中缩放，原本在局部空间的 `content_mask` 必须在 CPU 端按矩阵计算后同步至 GPU Scissor 栈。
* **难点 3（生命周期解耦）**：
  GPUI 当前设计为单窗口单 `Frame` 单 `Scene`，引入子场景需要重构 `Frame` 的生命周期，分离局部更新视图与全局合成视图。

### 3.3 路线评估
* **实现复杂度**：中高（预估工作量：1.5 ~ 2 周，涉及 HLSL 编译、D3D11 管线状态切换与调试）。
* **性能特征**：CPU 开销归零（无论包含多少万个图元，CPU 每帧仅更新一个 16 浮点数的 ConstantBuffer）。
* **局限**：对底层平台层（DirectX / Metal / Vulkan）具有强耦合性，多平台维护成本高。

---

## 4. 路线 3：离屏渲染为 GPU 贴图（Offscreen RenderTarget 缓存）

### 4.1 架构原理
现代图形设计软件（Figma、Miro 及 Webkit Compositor）的标准实现形态：
1. **内容与视口解耦**：
   - 100 张卡片以 `1.0x` 离屏栅格化一次，渲染并保存在一张独立的 GPU 纹理（`ID3D11Texture2D`，作为离屏 RenderTarget）中。
2. **画布渲染极度退化**：
   - 主画布每帧渲染时，**整个 100 张卡片阵列仅仅是一个带纹理的 Quad（仅 4 个顶点）**；
   - 缩放和平移完全等同于改变这 1 个 Quad 的纹理贴图采样坐标与屏幕映射位置；
   - GPU 依靠硬件双线性过滤（Bilinear Filtering）自动完成下采样抗锯齿与缩放。

### 4.2 架构支撑模块规划
1. **分块管理（Tile Management）**：
   超大画布（如 $4000 \times 6000$ 甚至更大）不能无节制申请超大单张纹理（容易爆显存且超过显卡最大纹理尺寸限制，一般为 16384px），需要引入 $512 \times 512$ 或 $1024 \times 1024$ 的虚拟分块网格（Tile Grid）。
2. **多级清晰度（LOD）与视口局部重绘**：
   在大幅度放大（`zoom > 1.0`）时，为避免位图边缘模糊，需结合视口按需向可见瓦片发起重采样指令。
3. **输入焦点穿透**：
   由于显示的是静态贴图，当卡片获得焦点（例如点击 `Textarea` 开始输入）时，需将该卡片从离屏贴图中抠除，动态提升为活动交互 DOM 节点叠加在画布上方。

### 4.3 路线评估
* **实现复杂度**：高（预估工作量：2 ~ 3 周，需要实现完整的离屏 FBO 交换、分块缓存与失效算法）。
* **性能特征**：极端稳健。即使画布内包含 100 万个字、50 万个矩形，主画布交互期间帧率始终锁死在显示器上限（280fps+）。

---

## 5. 三种路线横向对比与选型决策建议

| 维度 | 路线 1：连续内存块切片拷贝 | 路线 2：子场景 + GPU 顶点矩阵 | 路线 3：离屏 GPU 贴图缓存 (FBO) |
| :--- | :--- | :--- | :--- |
| **解决的核心矛盾** | 彻底消除 Dev 模式下的逐项循环分发与堆分配 | 消除 CPU 端图元传递与逐图元缩放遍历 | 将图元数量对渲染性能的影响彻底清零 |
| **Dev 帧率预期** | **100 ~ 140 fps** | **200 ~ 280 fps** | **恒定 280 fps**（锁帧） |
| **代码侵入层级** | 仅 `gpui-pre`（Scene / Window / View） | `gpui-pre` + `gpui-pre-windows` + HLSL | 底层渲染器 + 上层画布组件架构重构 |
| **预估工作量** | **0.5 ~ 1 天** | **1.5 ~ 2 周** | **2 ~ 3 周** |
| **文字矢量清晰度**| 绝对原生矢量（字形实时抗锯齿） | 绝对原生矢量（字形实时抗锯齿） | 缩小时优秀（双线性下采样），大倍率放大时依赖重采样 |
| **交互元素支持度**| 100% 支持（原生 Hitbox / 监听器不变） | 需映射鼠标坐标系 | 输入元素需按需提升渲染层 |

### 5.1 实施建议
1. **第一阶段（立竿见影）**：
   优先实现 **路线 1（连续内存块切片批量拷贝）**。只需在 `patches/gpui-pre` 的 `view.rs`、`window.rs`、`scene.rs` 调整索引记录与数组拼接方式，耗时极短，能立刻解决当前 Dev 模式下被 30ms 逐项循环卡住的问题。
2. **第二阶段（若需要支撑万级卡片与无限画布）**：
   参考 `span` 的经验，引入**世界坐标投影与视口剔除（Frustum Culling）**，使进入管线的图元数始终约束在视口范围内；或者在有超大密集概览需求时，将不活跃区域下沉到 **路线 3（贴图缓存）**。
