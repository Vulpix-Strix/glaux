use super::*;
use pretty_assertions::assert_eq;

#[test]
fn target_source_is_flipped_vertically() {
    let source = flipped_source();
    assert_eq!(
        (source.x, source.y, source.width, source.height),
        (0.0, 0.0, 1280.0, -960.0)
    );
}

#[test]
fn shader_declares_the_uniforms_the_host_sets() {
    assert!(SHADER_SOURCE.contains(&format!("uniform vec2 {SOURCE_SIZE};")));
    assert!(SHADER_SOURCE.starts_with("#version 330"));
}
