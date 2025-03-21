use eframe::egui::{Color32, Rounding, Stroke, Visuals};
use crate::models::config::AppTheme;

/// 主题管理器，负责设置和应用主题
pub struct ThemeManager;

impl ThemeManager {
    /// 根据选择的主题获取对应的视觉效果设置
    pub fn get_visuals(theme: &AppTheme) -> Visuals {
        match theme {
            AppTheme::Light => Self::light_theme(),
            AppTheme::Dark => Self::dark_theme(),
            AppTheme::NightBlue => Self::night_blue_theme(),
            AppTheme::Sepia => Self::sepia_theme(),
            AppTheme::Forest => Self::forest_theme(),
        }
    }

    /// 标准亮色主题
    fn light_theme() -> Visuals {
        let mut visuals = Visuals::light();
        visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(245, 245, 245);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(220, 220, 220));
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(240, 240, 240);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(200, 200, 200));
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(230, 230, 250);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(180, 180, 200));
        visuals.widgets.active.bg_fill = Color32::from_rgb(210, 210, 250);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(160, 160, 220));
        visuals.selection.bg_fill = Color32::from_rgb(144, 194, 231);
        visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(100, 150, 200));
        visuals.window_rounding = Rounding::same(8.0);
        visuals.window_shadow.extrusion = 8.0;
        visuals.window_fill = Color32::from_rgb(250, 250, 250);
        visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(210, 210, 210));
        visuals
    }

    /// 标准暗色主题
    fn dark_theme() -> Visuals {
        let mut visuals = Visuals::dark();
        visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(30, 30, 30);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(60, 60, 60));
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(45, 45, 45);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(80, 80, 80));
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(50, 50, 60);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(90, 90, 110));
        visuals.widgets.active.bg_fill = Color32::from_rgb(55, 55, 70);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(100, 100, 130));
        visuals.selection.bg_fill = Color32::from_rgb(45, 85, 130);
        visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(60, 120, 180));
        visuals.window_rounding = Rounding::same(8.0);
        visuals.window_shadow.extrusion = 10.0;
        visuals.window_fill = Color32::from_rgb(25, 25, 35);
        visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(60, 60, 80));
        visuals
    }

    /// 夜间蓝主题
    fn night_blue_theme() -> Visuals {
        let mut visuals = Visuals::dark();
        visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(18, 25, 40);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(40, 50, 70));
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(25, 35, 55);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(50, 65, 90));
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(35, 45, 70);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(60, 80, 120));
        visuals.widgets.active.bg_fill = Color32::from_rgb(45, 60, 90);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(80, 100, 150));
        visuals.selection.bg_fill = Color32::from_rgb(70, 90, 150);
        visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(90, 120, 200));
        visuals.window_rounding = Rounding::same(8.0);
        visuals.window_shadow.extrusion = 12.0;
        visuals.window_fill = Color32::from_rgb(15, 20, 35);
        visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(40, 55, 90));
        visuals.override_text_color = Some(Color32::from_rgb(220, 230, 245));
        visuals
    }

    /// 护眼模式/Sepia主题
    fn sepia_theme() -> Visuals {
        let mut visuals = Visuals::light();
        visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(250, 240, 220);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(230, 220, 200));
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(245, 235, 215);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(220, 210, 190));
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(235, 225, 205);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(210, 200, 180));
        visuals.widgets.active.bg_fill = Color32::from_rgb(225, 215, 195);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(200, 190, 170));
        visuals.selection.bg_fill = Color32::from_rgb(200, 175, 130);
        visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(180, 155, 110));
        visuals.window_rounding = Rounding::same(8.0);
        visuals.window_shadow.extrusion = 8.0;
        visuals.window_fill = Color32::from_rgb(250, 245, 230);
        visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(230, 220, 200));
        visuals.override_text_color = Some(Color32::from_rgb(80, 65, 40));
        visuals
    }

    /// 森林绿主题
    fn forest_theme() -> Visuals {
        let mut visuals = Visuals::dark();
        visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(25, 40, 30);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(45, 70, 50));
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(35, 55, 40);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(60, 90, 65));
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(45, 70, 50);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(75, 115, 80));
        visuals.widgets.active.bg_fill = Color32::from_rgb(55, 85, 60);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(90, 140, 95));
        visuals.selection.bg_fill = Color32::from_rgb(70, 130, 80);
        visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(90, 160, 100));
        visuals.window_rounding = Rounding::same(8.0);
        visuals.window_shadow.extrusion = 10.0;
        visuals.window_fill = Color32::from_rgb(20, 35, 25);
        visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(50, 80, 55));
        visuals.override_text_color = Some(Color32::from_rgb(210, 235, 215));
        visuals
    }

    /// 获取主题名称列表（用于显示在UI中）
    pub fn get_theme_names() -> Vec<(&'static str, AppTheme)> {
        vec![
            ("亮色主题", AppTheme::Light),
            ("暗黑主题", AppTheme::Dark),
            ("夜间蓝", AppTheme::NightBlue),
            ("护眼模式", AppTheme::Sepia),
            ("森林绿", AppTheme::Forest),
        ]
    }

    /// 根据主题获取适合该主题的强调色
    pub fn get_accent_color(theme: &AppTheme) -> Color32 {
        match theme {
            AppTheme::Light => Color32::from_rgb(66, 133, 244),
            AppTheme::Dark => Color32::from_rgb(75, 145, 250),
            AppTheme::NightBlue => Color32::from_rgb(86, 157, 255),
            AppTheme::Sepia => Color32::from_rgb(173, 124, 58),
            AppTheme::Forest => Color32::from_rgb(95, 188, 115),
        }
    }

    /// 获取状态颜色（成功、警告、错误等）
    pub fn get_status_colors() -> (Color32, Color32, Color32, Color32) {
        (
            Color32::from_rgb(76, 175, 80),  // 成功（绿色）
            Color32::from_rgb(255, 152, 0),  // 警告（橙色）
            Color32::from_rgb(244, 67, 54),  // 错误（红色）
            Color32::from_rgb(33, 150, 243), // 信息（蓝色）
        )
    }
} 