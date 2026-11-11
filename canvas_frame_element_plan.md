# `CanvasFrame` + `CanvasElement` 架构与实现规划

> **核心目标**：
> 1. 用 **`CanvasFrame`** 接管唯一的相机控制（`zoom` / `pan`），在内部建立统一的 **1x 世界坐标系** 与单层 `paint_layer`，并在 `paint` 末尾通过 `Scene::scale_since` 批量完成屏幕空间映射。
> 2. 用 **`CanvasElement`** 包装每个独立的画布元素（如单张卡片 `Entity<CardView>`），在 1x 世界坐标系下使用**绝对坐标定位**，并为每个元素赋予**自身的随动边界（Self-following Bounds & ContentMask）**以稳定命中 GPUI 的 View 缓存。
> 3. 暂不引入视口剔除（Frustum Culling），保持全量挂载、**交互即单卡 Dirty** 的极简模型：缩放/平移时 100% 命中切片缓存；点击/交互某张卡片时，仅该卡片（1/100）重跑 Layout 与 Paint，其余 99 张卡片继续走微秒级切片复用。

---

## 1. 整体架构图

```text
HelloWorld (Root View)
 └── CanvasFrame (管理唯一 zoom / pan，建立 1x 世界空间与外层 paint_layer)
      ├── CanvasElement #0 (world_pos=(0, 0), size=(380, 420))
      │    ├── 随动边界：bounds = Bounds(pivot + world_pos, size)
      │    ├── 随动遮罩：ContentMask = ContentMask { bounds } (1x 世界空间)
      │    └── Entity<CardView #0>.cached(style)  <-- 独立缓存单元 (交互仅 dirty 自身)
      │
      ├── CanvasElement #1 (world_pos=(404, 0), size=(380, 420))
      │    ├── 随动边界：bounds = Bounds(pivot + world_pos, size)
      │    ├── 随动遮罩：ContentMask = ContentMask { bounds } (1x 世界空间)
      │    └── Entity<CardView #1>.cached(style)  <-- 独立缓存单元
      │
      ├── ... (CanvasElement #2 .. #99)
      │
      └── [CanvasFrame::paint 末尾]
           └── Scene::scale_since(cp, pivot, zoom, pan)
                └── 将本帧所有图元及其随动 ContentMask 一次性缩放/平移至屏幕空间
```

---

## 2. 为什么“随动边界”能彻底解决缓存命中与裁剪矛盾？

在 GPUI 的 [`view.rs`](patches/gpui-pre/src/view.rs#L550-L556) 中，`Entity<T>::cached` 的缓存命中条件为：

```rust
element_state.cache_key.bounds == bounds
    && element_state.cache_key.content_mask == content_mask
    && element_state.cache_key.text_style == text_style
    && element_state.cache_key.opacity.to_bits() == opacity.to_bits()
    && !window.dirty_views.contains(&entity_id)
    && !window.refreshing
```

之前单体画布方案之所以被迫在 `window.rs` 硬编码 `4016 × 5986` 的全局 `ContentMask`，是因为相机缩放/移动会改变由窗口视口反推的 `content_mask`，从而击穿缓存。

换成 **`CanvasElement` 随动边界** 后：
1. **1x 世界坐标下的边界恒定**：
   每个 `CanvasElement` 的 `world_bounds = Bounds::new(frame_pivot + world_pos, size)`。当 `CanvasFrame` 发生 `zoom` 或 `pan` 时，`frame_pivot`、`world_pos`、`size` 均不发生任何变化。
2. **随动 `ContentMask` 隔离外部相机变化**：
   `CanvasElement` 在 `prepaint` 和 `paint` 进入子元素前，直接将当前 `window` 的 `ContentMask` 设为自身的 `world_bounds`（即 `ContentMask { bounds: world_bounds }`）。
   - 对内：`ViewElement` 检查 `cache_key.bounds` 和 `cache_key.content_mask` 时，看到的永远是卡片自己稳定的 `world_bounds`，**缩放与拖拽时 100% 稳定命中缓存**；若未来业务层拖动该卡片改变 `world_pos` 或调整 `size`，边界随之自动更新。
   - 对外：卡片内部图元携带该卡片的 `world_bounds` 作为 `content_mask`，在 `CanvasFrame::paint` 末尾经 `Scene::scale_since` 变换后，自动变成屏幕空间下正确的卡片裁剪框，**彻底告别硬编码大尺寸 `ref_size`**。

---

## 3. 核心组件设计（`src/main.rs`）

### 3.1 `CanvasFrame`：唯一相机控制容器

`CanvasFrame` 不再关心内部有多大（移除 `ref_size`），只负责维护相机参数 `(zoom, pan)` 并调度子元素列表。

```rust
pub struct CanvasFrame {
    zoom: f32,
    pan: Point<Pixels>,
    children: Vec<AnyElement>,
}

impl CanvasFrame {
    pub fn new(zoom: f32, pan: Point<Pixels>) -> Self {
        Self {
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            pan,
            children: Vec::new(),
        }
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.children.extend(children.into_iter().map(|c| c.into_any_element()));
        self
    }
}
```

#### `Element` 生命周期实现：
1. **`request_layout`**：
   - `CanvasFrame` 自身请求占满父容器视口（`style.size.width = relative(1.0); style.size.height = relative(1.0);`），从而拿到固定的容器左上角 `bounds.origin` 作为缩放枢轴 `pivot`。
   - 遍历 `self.children` 调用 `child.request_layout(window, cx)`，但**不将子元素的 `LayoutId` 挂入 Taffy 父节点**（或者传空切片 `&[]` 给 `window.request_layout(style, [], cx)`），因为每个 `CanvasElement` 会在自己的 `request_layout` 中通过 `layout_as_root` 独立完成定宽高布局，无需 Taffy 在画布层级做多节点联合约束求解。
2. **`prepaint`**：
   - 以 `bounds.origin` 为 `pivot`。
   - 在 `window.with_canvas_transform(pivot, self.zoom, self.pan, |window| ...)` 中（仅压入 `scaled_transform_stack` 供 Hitbox 记录逆变换）：
     - 遍历 `self.children`，调用 `child.prepaint_at(bounds.origin, window, cx)`。
3. **`paint`**：
   - 以 `bounds.origin` 为 `pivot`。
   - 画布整体建立一个足够覆盖全部世界坐标图元的全局图层边界 `canvas_bounds`，并以 `window.with_unclipped_content_mask` 与 `window.paint_layer(canvas_bounds, ...)` 包裹：
     - 确保该图层必定与父级视口相交，从而获得高于父级背景（例如根容器绿色背景）的基准 `DrawOrder`。
     - 同时该图层在世界空间下足够宽广，覆盖所有世界坐标下的子元素。
     - 遍历 `self.children`，调用 `child.paint(window, cx)`（由各个 `CanvasElement` 独立建立随动 `paint_layer`，使文字等内部元素稳定具有更高的 `DrawOrder`）。
   - `with_scaled_paint` 结束时，自动执行 `Scene::scale_since`，将所有子卡片本帧写入的图元及各自随动的 `content_mask` 统一按 `(pivot, zoom, pan)` 变换到屏幕空间。

---

### 3.2 `CanvasElement`：1x 世界坐标绝对定位与随动边界包装器

`CanvasElement` 负责将任意子元素（尤其是 `Entity<T>.cached(...)`）锚定在 1x 世界坐标的 `(world_pos, size)` 上，并提供随动裁剪边界。

```rust
pub struct CanvasElement {
    /// 1x 世界坐标系下相对于画布原点的偏移
    world_pos: Point<Pixels>,
    /// 1x 世界坐标系下的卡片固定宽高
    size: Size<Pixels>,
    /// 内部包裹的元素（通常为 Entity<CardView>.cached(style)）
    child: AnyElement,
}

impl CanvasElement {
    pub fn new(world_pos: Point<Pixels>, size: Size<Pixels>, child: impl IntoElement) -> Self {
        Self {
            world_pos,
            size,
            child: child.into_any_element(),
        }
    }
}
```

#### `Element` 生命周期实现：
1. **`request_layout`**：
   - 直接对 `self.child` 调用 `layout_as_root`：
     ```rust
     self.child.layout_as_root(
         size(
             AvailableSpace::Definite(self.size.width),
             AvailableSpace::Definite(self.size.height),
         ),
         window,
         cx,
     );
     ```
   - 当内部 `self.child` 是 `Entity<CardView>.cached(card_style)` 时，`ViewElement::request_layout` 只注册一个定宽高的单节点 `LayoutId`，**未 dirty 时根本不会调用 `CardView::render`**。
   - 返回一个默认空 `Style` 的 `LayoutId`（仅满足 `Element` 签名）。
2. **`prepaint`**：
   - 接收父级 `CanvasFrame` 传入的 `frame_origin`（即 `bounds.origin`）。
   - 计算当前元素在 1x 世界空间下的随动边界：
     ```rust
     let world_bounds = Bounds::new(
         point(frame_origin.x + self.world_pos.x, frame_origin.y + self.world_pos.y),
         self.size,
     );
     ```
   - 使用随动边界设置当前 `ContentMask`（不与视口相交截断，直接作为子树的 1x 局部遮罩）：
     ```rust
     window.with_unclipped_content_mask(ContentMask { bounds: world_bounds }, |window| {
         self.child.prepaint_at(world_bounds.origin, window, cx);
     });
     ```
3. **`paint`**：
   - 同样计算 `world_bounds`，并在随动 `ContentMask { bounds: world_bounds }` 与 `paint_layer(world_bounds)` 下调用：
     ```rust
     window.with_unclipped_content_mask(ContentMask { bounds: world_bounds }, |window| {
         window.paint_layer(world_bounds, |window| {
             self.child.paint(window, cx);
         });
     });
     ```

---

### 3.3 独立卡片实体 `CardView`（替代单体 `CardsCanvas`）

将原来的 `CardsCanvas`（1 个 View 渲染 100 张卡片）拆分为 100 个独立的 `Entity<CardView>`：

```rust
struct CardView {
    idx: usize,
    button_red: bool,
    note: Option<Entity<TextareaState>>,
}

impl Render for CardView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let button_color = if self.button_red { red(500) } else { blue(500) };
        // 渲染单张卡片的 Markdown + Button
        // 点击 Button 时：
        // this.button_red = !this.button_red;
        // cx.notify(); <-- 仅标记当前这一张 CardView 为 dirty！
    }
}
```

在 `HelloWorld` 中持有卡片列表：
```rust
struct CanvasItem {
    world_pos: Point<Pixels>,
    size: Size<Pixels>,
    view: Entity<CardView>,
}
```
在 `HelloWorld::render` 中组装：
```rust
CanvasFrame::new(zoom, pan).children(self.items.iter().map(|item| {
    let mut style = StyleRefinement::default();
    style.size.width = Some(item.size.width.into());
    style.size.height = Some(item.size.height.into());
    CanvasElement::new(
        item.world_pos,
        item.size,
        item.view.clone().cached(style),
    )
}))
```

---

## 4. GPUI 底层配合修改点（`patches/gpui-pre/src/window.rs`）

为了支撑 `CanvasFrame` + `CanvasElement` 架构，并彻底消除硬编码与缓存复用后的交互坐标偏移，需对 `window.rs` 做以下 4 处精准调整：

### 4.1 新增 `with_unclipped_content_mask` 与 `with_canvas_transform`（清理硬编码 `4016 × 5986`）
- **现状问题**：`window.rs #3466` 的 `with_scaled_content_mask` 中硬编码了 `ref_size = (4016.0, 5986.0)`。
- **修改方案**：
  1. 提供 `pub fn with_unclipped_content_mask<R>(&mut self, mask: ContentMask<Pixels>, f: impl FnOnce(&mut Self) -> R) -> R`，直接 push/pop `content_mask_stack`（供 `CanvasElement` 设置随动边界遮罩）。
  2. 提供 `pub fn with_canvas_transform<R>(&mut self, pivot: Point<Pixels>, scale: f32, pan: Point<Pixels>, f: impl FnOnce(&mut Self) -> R) -> R`，在 `CanvasFrame` 的 `prepaint` 和 `paint` 中仅 push/pop `scaled_transform_stack`。
  3. `with_scaled_paint` 内部改为调用 `with_canvas_transform` + `Scene::scale_since`，移除硬编码的 `4016 × 5986`。

### 4.2 在 `reuse_prepaint` 中更新复用 Hitbox 的相机变换（关键！）
- **现状问题**：[`window.rs #4399`](patches/gpui-pre/src/window.rs#L4399) 在命中缓存执行 `reuse_prepaint` 时，直接 `.cloned()` 了上一帧的 `Hitbox`。当相机发生缩放或平移后，复用的 `Hitbox.transform` 仍然是旧的 `(pivot, scale, pan)`，会导致缩放/平移后鼠标无法在正确的新位置点中卡片按钮。
- **修改方案**：
  ```rust
  let current_transform = self.current_scaled_transform();
  self.next_frame.hitboxes.extend(
      self.rendered_frame.hitboxes[range.start.hitboxes_index..range.end.hitboxes_index]
          .iter()
          .map(|hitbox| {
              let mut hitbox = hitbox.clone();
              hitbox.transform = current_transform.clone();
              hitbox
          }),
  );
  ```

### 4.3 在 `on_mouse_event` / `reuse_paint` 中动态读取当前帧相机变换（关键！）
- **现状问题**：[`window.rs #5884`](patches/gpui-pre/src/window.rs#L5884) 在注册 `on_mouse_event` 时，将录制帧的 `let transform = self.current_scaled_transform();` 捕获进了闭包。当后续帧缩放/平移并命中缓存走 `reuse_paint` 时，搬运过来的闭包内部依然拿着首帧的 `transform` 映射鼠标坐标，导致按钮点击判定坐标错位。
- **修改方案**：
  将 `mouse_listeners` 存储结构或捕获方式改为保留可变变换（或在 `Frame::mouse_listeners` 旁同步记录每条 listener 所属的 `Option<Rc<[(Point<Pixels>, f32, Point<Pixels>)]>>`），在 `reuse_paint` 搬运 `mouse_listeners` 时，用当前帧的 `self.current_scaled_transform()` 刷新该变换。

### 4.4 修复 `Window::blur` 无焦点时的冗余 `self.refresh()`（关键！）
- **现状问题**：[`window.rs #2697`](patches/gpui-pre/src/window.rs#L2697) 中，`window.blur(cx)` 即便在 `self.focus.is_none()` 时也会无条件调用 `self.refresh()`，将 `window.refreshing` 置为 `true`。而 [`view.rs #556`](patches/gpui-pre/src/view.rs#L556) 检查 `&& !window.refreshing`，导致只要点击画布触发 `window.blur(cx)`，**100 张卡片会同时因为 `refreshing=true` 全部丢失缓存**！
- **修改方案**：
  在 `Window::blur` 中，仅当原本存在焦点（`self.focus.is_some()`）时才清理并触发刷新，或者移除无焦点时的全局 `self.refresh()`：
  ```rust
  pub fn blur(&mut self, cx: &mut App) {
      self.clear_pending_keystrokes(cx);
      if !self.focus_enabled {
          return;
      }
      if self.focus.take().is_some() {
          self.focus_generation = self.focus_generation.wrapping_add(1);
          self.refresh();
      }
  }
  ```

---

## 5. 实施步骤清单

1. **Step 1（底层 `window.rs` 修正）**：
   - 新增 `with_unclipped_content_mask` 与 `with_canvas_transform`，清理 `with_scaled_content_mask` 中的硬编码尺寸。
   - 在 `reuse_prepaint` 与 `reuse_paint` 中刷新复用的 `Hitbox` 和 `MouseListener` 的 `scaled_transform`。
   - 修正 `Window::blur` 仅在真正失去焦点时触发 `refresh()`。
2. **Step 2（实现 `CanvasFrame` 与 `CanvasElement`）**：
   - 在 `src/main.rs` 中实现 `CanvasFrame`（管理唯一 `zoom`、`pan`、外层 `paint_layer` 与 `with_scaled_paint`）。
   - 在 `src/main.rs` 中实现 `CanvasElement`（管理 1x 世界坐标 `world_pos`、`size`、`layout_as_root` 与随动 `ContentMask`）。
3. **Step 3（拆分 `CardView` 独立实体）**：
   - 将 10×10 网格中的每张卡片实例化为独立的 `Entity<CardView>`，按行列计算出各自的 1x 世界坐标 `world_pos` 并装入 `CanvasElement`。
4. **Step 4（性能与交互验证）**：
   - **拖拽与缩放**：验证 100 张卡片 0 Cache Miss，全部走 `replay_cached` 切片拷贝。
   - **缩放/平移后点击卡片按钮**：验证鼠标在任意缩放比例和平移偏移下均能精准 Hover 和点击按钮。
   - **交互单帧耗时验证**：验证点击某张卡片的“切换颜色”按钮时，终端仅打印 **1 次** `CACHE MISS! dirty=true`（仅重绘该卡片），其余 99 张卡片全部命中缓存，单帧耗时从之前的 120ms（Dev）/ 28ms（Release）降至与平时拖拽相当的极低水平（Dev ~5ms / Release ~1.5ms）。
