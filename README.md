# gpui_zoom_test

`gpui-pre` & `gpui-base` in `./patches` are both pulled from `cargo clone` and modified.

---

## 画布缓存隔离机制（绕过 refreshing 沿用缓存）

### 1. 背景与核心问题
GPUI 原生设计中，`Entity::cached` 的命中条件为：
```rust
cache_key.bounds == bounds
    && cache_key.content_mask == content_mask
    && cache_key.text_style == text_style
    && cache_key.opacity.to_bits() == opacity.to_bits()
    && !window.dirty_views.contains(&entity_id)
    && !window.refreshing // <-- 关键限制
```
而在实际应用中，许多非数据交互事件（如窗口移动、鼠标移入移出、窗口失焦 `blur`、焦点切换 `focus`、按钮 `:active` 伪类、`pending_mouse_down` 点击追踪等）都会在底层调用 `window.refresh()`。

这会导致 `window.refreshing = true`，从而引发**全量缓存连坐击穿**：即使 100 张卡片的位置、数据、样式完全没有改变，也会在每一帧全部触发 `CACHE MISS! dirty=false, refreshing=true`。

### 2. 实现方式
在 `Window` 上增加作用域级刷新状态接管方法（`patches/gpui-pre/src/window.rs`）：
```rust
pub fn with_view_cache_refreshing<R>(
    &mut self,
    refreshing: bool,
    f: impl FnOnce(&mut Self) -> R,
) -> R {
    let previous = std::mem::replace(&mut self.refreshing, refreshing);
    let ret = f(self);
    self.refreshing = previous;
    ret
}
```
并在相机顶层容器 `CanvasFrame::prepaint`（`src/canvas/frame.rs`）遍历卡片子元素时，为子树屏蔽外层的 `refreshing` 状态：
```rust
window.with_canvas_transform(pivot, zoom, pan, |window| {
    // 隔离外层全局 refresh，让子卡片只认自身的 dirty_views 与 cache_key
    window.with_view_cache_refreshing(false, |window| {
        for child in &mut self.children {
            child.prepaint_at(pivot, window, cx);
        }
    });
});
```

### 3. 达到的效果
- **平移 / 缩放 / 窗口拖拽 / 失焦**：全量 100 张卡片 100% 命中切片缓存（0 Cache Miss），dev 模式下稳定 100~120 FPS。
- **业务交互**：仅被点击/操作的单张卡片触发 `dirty=true` 重绘（1/100 Cache Miss），其余 99 张卡片不受任何波及，继续复用上帧图元。

---

## 沿用缓存的相关注意事项（Edge Cases）

1. **响应式数据更新必须显式 `cx.notify()`**
   - 卡片在命中缓存时不会重新执行 `render()`。
   - 所有改变卡片外观的状态，必须归属于卡片自身的 Model/Entity，并在发生变化时调用卡片的 `cx.notify()`，不能依赖父级重新 render 时的传参刷新。

2. **卡片内部原生 `:hover` / `:active` 伪类变色被跳过**
   - GPUI 原生匿名 `div().hover(...)` / `.active(...)` 依赖 `window.refresh()` 来更新视觉。在屏蔽 `refreshing` 后，仅悬浮或按下时不会自动重绘当前卡片。
   - 若卡片内需要即时的 Hover / Active 视觉反馈，需在卡片 View 内维护状态并调用当前卡片的局部 `cx.notify()`。

3. **全局主题（Theme）切换**
   - 若通过 `window.refresh()` 广播全局明暗主题切换，卡片可能不会立刻变色。
   - 解决方案：切换全局主题时，遍历或广播通知所有卡片执行一次 `cx.notify()`。

4. **输入框光标与动画**
   - 若未来在卡片内嵌入 `TextInput`（光标闪烁）或 GPUI `Animation`（连续帧插值），其底层通常依赖 `request_animation_frame` / `refresh`。
   - 动画或输入态期间，该卡片自身需保持连续的 `cx.notify()` 驱动。
