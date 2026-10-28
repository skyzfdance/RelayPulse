use super::*;

#[test]
fn active_icon_is_blue_and_both_states_preserve_the_existing_logo_alpha_mask() {
    let source = include_bytes!("../../../icons/tray-logo.rgba");
    for (state, color) in [
        (TrayState::Active, [22, 119, 255]),
        (TrayState::Default, [115, 115, 115]),
    ] {
        let image = icon(state);
        assert_eq!((image.width(), image.height()), (64, 64));
        assert_eq!(image.rgba().len(), source.len());
        for (pixel, original) in image.rgba().chunks_exact(4).zip(source.chunks_exact(4)) {
            assert_eq!(pixel[3], original[3]);
            if original[3] != 0 {
                assert_eq!(pixel[..3], color);
            }
        }
    }
}
