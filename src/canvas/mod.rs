//! 画布相机容器 [`CanvasFrame`] 与世界坐标子元素 [`CanvasElement`]。

mod element;
mod frame;
mod overlay;

pub use element::CanvasElement;
pub use frame::CanvasFrame;
pub use overlay::{ResizeAxis, SelectionOverlay};

/// 默认最小缩放（与 demo 一致，接入时可覆盖）。
pub const DEFAULT_MIN_ZOOM: f32 = 0.1;
/// 默认最大缩放。
pub const DEFAULT_MAX_ZOOM: f32 = 1.0;
