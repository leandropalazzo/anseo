//! Story 25.1 — accent color validation for org branding.
//! Validates that a hex color provides WCAG AA contrast against white (#FFFFFF).
//! This ensures the color is in a "contrast-safe range" as required by UX-DR-P4-2.

/// Minimum contrast ratio for WCAG AA (normal text).
const WCAG_AA_CONTRAST: f64 = 4.5;

/// Validate that `hex` is a valid #RRGGBB color with WCAG AA contrast against white.
/// Returns Ok(()) if valid, Err(reason) if not.
pub fn validate_accent_hex(hex: &str) -> Result<(), &'static str> {
    if !hex.starts_with('#') || hex.len() != 7 {
        return Err("color must be in #RRGGBB format");
    }
    let Ok(r) = u8::from_str_radix(&hex[1..3], 16) else {
        return Err("invalid hex digits");
    };
    let Ok(g) = u8::from_str_radix(&hex[3..5], 16) else {
        return Err("invalid hex digits");
    };
    let Ok(b) = u8::from_str_radix(&hex[5..7], 16) else {
        return Err("invalid hex digits");
    };

    let contrast = contrast_against_white(r, g, b);
    if contrast < WCAG_AA_CONTRAST {
        return Err("accent color must have at least 4.5:1 contrast against white (WCAG AA)");
    }
    Ok(())
}

fn linear(c: u8) -> f64 {
    let s = c as f64 / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

fn contrast_against_white(r: u8, g: u8, b: u8) -> f64 {
    let l = relative_luminance(r, g, b);
    // White luminance = 1.0
    (1.0 + 0.05) / (l + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_fails_contrast() {
        assert!(validate_accent_hex("#FFFFFF").is_err());
    }

    #[test]
    fn black_passes_contrast() {
        assert!(validate_accent_hex("#000000").is_ok());
    }

    #[test]
    fn dark_blue_passes() {
        // Anseo default accent in light mode: oklch(0.52 0.14 238) ≈ #1a5fb4
        assert!(validate_accent_hex("#1a5fb4").is_ok());
    }

    #[test]
    fn invalid_format_rejected() {
        assert!(validate_accent_hex("1a5fb4").is_err()); // no #
        assert!(validate_accent_hex("#GGGGGG").is_err()); // invalid hex
        assert!(validate_accent_hex("#FFF").is_err()); // short form
    }
}
