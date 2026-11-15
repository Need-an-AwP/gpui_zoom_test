use crate::canvas::{Camera, CanvasElement, CanvasFrame, ResizeAxis, SelectionOverlay};
use crate::card::{CanvasItem, CardView, MIN_CARD_SIZE};
use crate::hud::{CardOption, CardSelectState, build_card_select, camera_hud};
use gpui_fps::fps_monitor;
use gpui_kit::component::searchable_list::SearchableVec;
use gpui_kit::component::select::SelectEvent;
use gpui_kit::component::{black, white};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

#[derive(Clone, Copy)]
struct ResizeDrag {
    axis: ResizeAxis,
    index: usize,
    start_mouse: Point<Pixels>,
    start_size: Size<Pixels>,
}

pub struct CanvasWorkspace {
    camera: Camera,
    /// 最近一帧的窗口客户区，选择卡片时用来对准视口中心。
    viewport_size: Size<Pixels>,
    /// 相机 HUD 里的卡片选择器。单向数据流：选择 -> 相机，相机变化不会回写选择器。
    card_select: Entity<CardSelectState>,
    _card_select_sub: Subscription,
    drag_start: Option<(Point<Pixels>, Point<Pixels>)>,
    items: Vec<CanvasItem>,
    selected_index: Option<usize>,
    resizing: Option<ResizeDrag>,
}

impl CanvasWorkspace {
    pub fn new(
        items: Vec<CanvasItem>,
        grid_cols: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let card_select = build_card_select(items.len(), grid_cols, window, cx);
        let sub = cx.subscribe(
            &card_select,
            |this, _, event: &SelectEvent<SearchableVec<CardOption>>, cx| {
                let SelectEvent::Confirm(Some(index)) = event else {
                    return;
                };
                this.move_camera_to_card(*index, cx);
                cx.notify();
            },
        );
        Self {
            card_select,
            _card_select_sub: sub,
            camera: Camera::new(1.0, Point::default()),
            viewport_size: window.viewport_size(),
            drag_start: None,
            items,
            selected_index: Some(0),
            resizing: None,
        }
    }

    /// 选中卡片并平移相机，使卡片中心落在视口中心；缩放保持不变。尺寸尚未测量则只选中、不移动。
    fn move_camera_to_card(&mut self, index: usize, cx: &App) {
        let Some(item) = self.items.get(index) else {
            return;
        };
        self.selected_index = Some(index);
        let Some(size) = item.view.read(cx).resolved else {
            return;
        };
        let card_center = item.world_pos + size.center();
        self.camera
            .animate_center_on_world_point(card_center, self.viewport_size);
    }

    /// 解析文本并设置 pan.x；非法输入忽略。
    #[allow(dead_code)]
    fn commit_hud_x(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Ok(val) = text.trim().parse::<f32>() {
            if val.is_finite() {
                self.camera.pan_to(point(px(val), self.camera.pan().y));
            }
        }
        cx.notify();
    }

    /// 解析文本并设置 pan.y；非法输入忽略。
    #[allow(dead_code)]
    fn commit_hud_y(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Ok(val) = text.trim().parse::<f32>() {
            if val.is_finite() {
                self.camera.pan_to(point(self.camera.pan().x, px(val)));
            }
        }
        cx.notify();
    }

    /// 解析文本并以窗口中心为锚点缩放；非法输入忽略。
    #[allow(dead_code)]
    fn commit_hud_zoom(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(val) = text.trim().parse::<f32>() {
            if val.is_finite() {
                let anchor = window.viewport_size().center();
                self.camera.zoom_to(val, anchor);
            }
        }
        cx.notify();
    }
}

impl Render for CanvasWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.camera.tick_pan(std::time::Instant::now()) {
            window.request_animation_frame();
        }
        self.viewport_size = window.viewport_size();
        let zoom = self.camera.zoom();
        let pan = self.camera.pan();

        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .bg(linear_gradient(
                        135.0,
                        linear_color_stop(black(), 0.2),
                        linear_color_stop(white(), 0.8),
                    ))
                    // .when(is_dragging, |this| this.cursor(CursorStyle::Crosshair))// not working on windows
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _, window, cx| {
                            window.blur(cx);
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            window.blur(cx);
                            let pan = this.camera.pan();
                            this.camera.pan_to(pan);
                            this.drag_start = Some((event.position, pan));
                            cx.notify();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Middle,
                        cx.listener(|this, _event: &MouseUpEvent, _, cx| {
                            this.drag_start = None;
                            cx.notify();
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Middle,
                        cx.listener(|this, _event: &MouseUpEvent, _, cx| {
                            this.drag_start = None;
                            cx.notify();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _event: &MouseUpEvent, _, _| {
                            this.resizing = None;
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _event: &MouseUpEvent, _, _| {
                            this.resizing = None;
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        if let Some(drag) = this.resizing {
                            if event.pressed_button != Some(MouseButton::Left) {
                                this.resizing = None;
                                return;
                            }
                            // 屏幕位移换算回世界位移；世界坐标 (world_pos) 不变，只改尺寸。
                            let delta = (event.position - drag.start_mouse) / this.camera.zoom();
                            let mut new_size = drag.start_size;
                            match drag.axis {
                                ResizeAxis::Right => {
                                    new_size.width =
                                        (drag.start_size.width + delta.x).max(MIN_CARD_SIZE.0);
                                }
                                ResizeAxis::Bottom => {
                                    new_size.height =
                                        (drag.start_size.height + delta.y).max(MIN_CARD_SIZE.1);
                                }
                            }
                            if let Some(item) = this.items.get(drag.index) {
                                let view = item.view.clone();
                                view.update(cx, |card, cx| card.resize(new_size, cx));
                            }
                            cx.notify();
                            return;
                        }
                        if let Some((start_pos, start_pan)) = this.drag_start {
                            if event.pressed_button == Some(MouseButton::Middle) {
                                let new_pan = start_pan + (event.position - start_pos);
                                if this.camera.pan() != new_pan {
                                    this.camera.pan_to(new_pan);
                                    cx.notify();
                                }
                            } else {
                                this.drag_start = None;
                                cx.notify();
                            }
                        }
                    }))
                    .on_scroll(cx.listener(|this, event: &ScrollEvent, _, cx| {
                        let delta_y = match event.delta {
                            ScrollDelta::Pixels(p) => p.y.as_f32(),
                            ScrollDelta::Lines(l) => {
                                if event.modifiers.control {
                                    l.y * 24.0
                                } else {
                                    l.y / 10.0 * 24.0
                                }
                            }
                        };
                        let factor = (1.0 + delta_y * 0.002).clamp(0.8, 1.25);
                        let k1 = this.camera.zoom() * factor;
                        this.camera.zoom_to(k1, event.position);
                        cx.notify();
                    }))
                    .child(
                        CanvasFrame::new(zoom, pan).children(self.items.iter().enumerate().map(
                            |(idx, item)| {
                                let resolved = item.view.read(cx).resolved;
                                let card = match resolved {
                                    Some(size) => item
                                        .view
                                        .clone()
                                        .cached(CardView::cached_style(size))
                                        .into_any_element(),
                                    None => item.view.clone().into_any_element(),
                                };
                                let is_selected = self.selected_index == Some(idx);
                                // 外层包装节点在 Capture 阶段接管左/右键：
                                // 未选中时只负责选中并截断事件，选中后放行给 CardView。
                                // CardView 本身不感知选中态，缓存不受影响。中键仍留给画布平移。
                                let child = div().flex().flex_col().child(card).when(
                                    !is_selected,
                                    |this| {
                                        this.capture_any_mouse_down(cx.listener(
                                            move |this, event: &MouseDownEvent, _, cx| {
                                                if event.button == MouseButton::Middle {
                                                    return;
                                                }
                                                cx.stop_propagation();
                                                if event.button == MouseButton::Left
                                                    && this.selected_index != Some(idx)
                                                {
                                                    this.selected_index = Some(idx);
                                                    cx.notify();
                                                }
                                            },
                                        ))
                                    },
                                );
                                let view = item.view.clone();
                                CanvasElement::new(item.world_pos, child).on_measured(
                                    move |measured, cx| {
                                        // 只收下第一次内容测量。之后空缓存节点的回声尺寸不再回写。
                                        if view.read(cx).resolved.is_some() {
                                            return false;
                                        }
                                        view.update(cx, |card, _cx| {
                                            card.resolved = Some(measured);
                                        });
                                        true
                                    },
                                )
                            },
                        )),
                    )
                    .children(self.selected_index.and_then(|idx| {
                        let item = self.items.get(idx)?;
                        let resolved = item.view.read(cx).resolved?;
                        let this = cx.entity();
                        Some(
                            SelectionOverlay::new(item.world_pos, resolved, zoom, pan)
                                .on_resize_start(move |axis, event, _, cx| {
                                    this.update(cx, |this, _| {
                                        this.resizing = Some(ResizeDrag {
                                            axis,
                                            index: idx,
                                            start_mouse: event.position,
                                            start_size: resolved,
                                        });
                                    });
                                }),
                        )
                    })),
            )
            .child(div().absolute().top_0().right_0().child(
                fps_monitor(window, cx).frame_budget(std::time::Duration::from_nanos(8_333_333)),
            ))
            .child(camera_hud(&self.camera, &self.card_select))
    }
}
