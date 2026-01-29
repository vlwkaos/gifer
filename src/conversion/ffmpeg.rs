use crate::config::Settings;

/// Build the FFmpeg filter chain for high-quality GIF conversion
/// Uses palette generation for better color quality
pub fn build_filter_chain(settings: &Settings) -> String {
    let fps = settings.fps.value();
    let dither = settings.quality.dither_option();

    // Build base filters with fps
    let base_filters = if let Some(scale) = settings.scale.ffmpeg_scale() {
        format!("fps={fps},{scale}")
    } else {
        format!("fps={fps}")
    };

    format!(
        "[0:v]{base_filters},split[a][b];\
         [a]palettegen=stats_mode=diff[palette];\
         [b][palette]paletteuse=dither={dither}"
    )
}

/// Get the -loop argument value for the GIF
pub fn get_loop_arg(settings: &Settings) -> String {
    settings.loop_count.ffmpeg_loop_value().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FpsPreset, LoopCount, Quality, Scale};

    #[test]
    fn test_build_filter_chain_with_scale() {
        let mut settings = Settings::default();
        settings.scale = Scale::H480;
        settings.fps = FpsPreset::Fps10;
        settings.quality = Quality::Medium;

        let filter = build_filter_chain(&settings);
        assert!(filter.contains("fps=10"));
        assert!(filter.contains("scale=-1:480"));
        assert!(filter.contains("palettegen"));
        assert!(filter.contains("sierra2_4a"));
    }

    #[test]
    fn test_build_filter_chain_original() {
        let mut settings = Settings::default();
        settings.scale = Scale::Original;
        settings.fps = FpsPreset::Fps15;
        settings.quality = Quality::High;

        let filter = build_filter_chain(&settings);
        assert!(filter.contains("fps=15"));
        assert!(!filter.contains("scale="));
        assert!(filter.contains("floyd_steinberg"));
    }

    #[test]
    fn test_build_filter_chain_percent() {
        let mut settings = Settings::default();
        settings.scale = Scale::Percent50;
        settings.fps = FpsPreset::Fps10;

        let filter = build_filter_chain(&settings);
        assert!(filter.contains("iw*0.5:ih*0.5"));
    }

    #[test]
    fn test_loop_arg() {
        let mut settings = Settings::default();

        settings.loop_count = LoopCount::Infinite;
        assert_eq!(get_loop_arg(&settings), "0");

        settings.loop_count = LoopCount::Once;
        assert_eq!(get_loop_arg(&settings), "-1");

        settings.loop_count = LoopCount::Count(3);
        assert_eq!(get_loop_arg(&settings), "3");
    }
}
