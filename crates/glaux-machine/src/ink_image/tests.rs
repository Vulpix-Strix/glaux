use super::*;
use pretty_assertions::assert_eq;

#[test]
fn pixels_are_row_major_and_transparent_outside() {
    let image = InkImage::new(2, 2, vec![0, 1, 2, 3]).unwrap();
    assert_eq!(
        [
            image.pixel(0, 0),
            image.pixel(1, 0),
            image.pixel(0, 1),
            image.pixel(1, 1)
        ],
        [0, 1, 2, 3]
    );
    assert_eq!(image.pixel(2, 0), 0);
    assert_eq!(image.pixel(0, 2), 0);
}

#[test]
fn rejects_size_mismatches_and_unknown_slots() {
    assert_eq!(
        InkImage::new(2, 2, vec![0; 3]),
        Err(InkImageError::Length {
            width: 2,
            height: 2,
            expected: 4,
            actual: 3
        })
    );
    assert_eq!(
        InkImage::new(2, 1, vec![1, 4]),
        Err(InkImageError::Slot { index: 1, slot: 4 })
    );
}
