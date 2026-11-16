mod app;
mod canvas;
mod card;
mod hud;

use app::CanvasWorkspace;
use card::{CARD_WIDTH, CanvasItem, CardView};
use gpui_kit::*;

const GRID_COLS: usize = 10;
const GRID_ROWS: usize = 10;
const GRID_STEP_X: f32 = CARD_WIDTH + 10.;
/// 初始行距。卡片高度由布局决定，不写进卡片样式。
const GRID_STEP_Y: f32 = 590.;

/// 桌面端由二进制 `main` 调用。wasm 由模块加载时的 `start` 调用。
pub fn run() {
    application().with_assets(asset_source()).run(|cx| {
        #[cfg(target_family = "wasm")]
        if !load_fonts(cx) {
            return;
        }
        init(cx);
        #[cfg(target_family = "wasm")]
        apply_web_theme_fonts(cx);

        open_window(window_options(cx), cx, |window, cx| {
            let origin = point(px(40.), px(40.));
            let mut items = Vec::with_capacity(GRID_COLS * GRID_ROWS);
            for row in 0..GRID_ROWS {
                for col in 0..GRID_COLS {
                    let world_pos = point(
                        origin.x + px(col as f32 * GRID_STEP_X),
                        origin.y + px(row as f32 * GRID_STEP_Y),
                    );
                    items.push(CanvasItem {
                        world_pos,
                        view: cx.new(|_| CardView::new()),
                    });
                }
            }
            cx.new(|cx| CanvasWorkspace::new(items, GRID_COLS, window, cx))
        })
        .expect("Failed to open window");
    });
}

/// 浏览器加载 wasm 模块时调用。不要再提供 wasm 二进制入口，否则会和这里的启动函数撞车。
#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    gpui_kit::platform::web_init();
    run();
}

#[cfg(target_family = "wasm")]
gpui_kit::assets::icon_assets!(WebAssets, [ChevronDown, Check, Close, Search, Inbox]);

fn asset_source() -> impl AssetSource {
    #[cfg(target_family = "wasm")]
    {
        WebAssets
    }
    #[cfg(not(target_family = "wasm"))]
    {
        assets::Assets
    }
}

fn window_options(cx: &App) -> WindowOptions {
    #[cfg(target_family = "wasm")]
    {
        // 尺寸跟 canvas 的 CSS 走。写死窗口 bounds 会在 web 平台上变成固定像素。
        let _ = cx;
        WindowOptions::default()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let bounds = Bounds::centered(None, size(px(1280.), px(720.)), cx);
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("canvas frame demo".into()),
                ..TitlebarOptions::default()
            }),
            ..Default::default()
        }
    }
}

#[cfg(target_family = "wasm")]
fn apply_web_theme_fonts(cx: &mut App) {
    let theme = gpui_kit::component::Theme::global_mut(cx);
    theme.font_family = "Noto Sans SC".into();
    theme.mono_font_family = "Noto Sans".into();
}

/// 按 span 方式在打开窗口前注册内置 Noto 字体（web 端默认字体库为空）。
#[cfg(target_family = "wasm")]
fn load_fonts(cx: &App) -> bool {
    use std::borrow::Cow;

    let fonts = [
        Cow::Borrowed(include_bytes!("../web/fonts/Noto_Sans/static/NotoSans-Regular.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../web/fonts/Noto_Sans/static/NotoSans-Italic.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../web/fonts/Noto_Sans/static/NotoSans-SemiBold.ttf").as_slice()),
        Cow::Borrowed(
            include_bytes!("../web/fonts/Noto_Sans/static/NotoSans-SemiBoldItalic.ttf").as_slice(),
        ),
        Cow::Borrowed(include_bytes!("../web/fonts/Noto_Sans/static/NotoSans-Bold.ttf").as_slice()),
        Cow::Borrowed(
            include_bytes!("../web/fonts/Noto_Sans/static/NotoSans-BoldItalic.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../web/fonts/Noto_Sans_SC/static/NotoSansSC-Thin.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../web/fonts/Noto_Sans_SC/static/NotoSansSC-ExtraLight.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../web/fonts/Noto_Sans_SC/static/NotoSansSC-Light.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../web/fonts/Noto_Sans_SC/static/NotoSansSC-Regular.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../web/fonts/Noto_Sans_SC/static/NotoSansSC-Medium.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../web/fonts/Noto_Sans_SC/static/NotoSansSC-SemiBold.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../web/fonts/Noto_Sans_SC/static/NotoSansSC-Bold.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../web/fonts/Noto_Sans_SC/static/NotoSansSC-ExtraBold.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../web/fonts/Noto_Sans_SC/static/NotoSansSC-Black.ttf").as_slice(),
        ),
        Cow::Borrowed(
            include_bytes!("../web/fonts/Noto_Color_Emoji/NotoColorEmoji-Regular.ttf").as_slice(),
        ),
    ];
    if let Err(error) = cx.text_system().add_fonts(fonts.into()) {
        eprintln!("failed to load application fonts: {error:#}");
        return false;
    }
    true
}
