use super::*;
use pretty_assertions::assert_eq;

#[test]
fn queue_is_first_in_first_out() {
    let mut queue = SampleQueue::new(8);
    assert_eq!(queue.push(&[1, 2, 3]), 0);
    assert_eq!(queue.push(&[4]), 0);

    let mut out = [0; 2];
    assert_eq!(queue.fill(&mut out), 0);
    assert_eq!(out, [1, 2]);
    assert_eq!(queue.fill(&mut out), 0);
    assert_eq!(out, [3, 4]);
}

#[test]
fn underrun_pads_with_silence() {
    let mut queue = SampleQueue::new(8);
    queue.push(&[5, 6]);

    let mut out = [9; 5];
    assert_eq!(queue.fill(&mut out), 3);
    assert_eq!(out, [5, 6, 0, 0, 0]);

    assert_eq!(queue.fill(&mut out), 5);
    assert_eq!(out, [0; 5]);
}

#[test]
fn overflow_drops_the_oldest_samples() {
    let mut queue = SampleQueue::new(4);
    queue.push(&[1, 2, 3]);
    assert_eq!(queue.push(&[4, 5, 6]), 2);

    let mut out = [0; 4];
    assert_eq!(queue.fill(&mut out), 0);
    assert_eq!(out, [3, 4, 5, 6]);
}
