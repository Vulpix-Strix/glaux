use super::*;
use pretty_assertions::assert_eq;

#[test]
fn evicts_oldest_entry_past_the_limit() {
    let mut scrollback = Scrollback::default();
    for n in 0..SCROLLBACK_LIMIT + 3 {
        scrollback.push(Entry::Output(n.to_string()));
    }
    assert_eq!(scrollback.iter().len(), SCROLLBACK_LIMIT);
    assert_eq!(scrollback.iter().next(), Some(&Entry::Output("3".into())));
    assert_eq!(
        scrollback.iter().next_back(),
        Some(&Entry::Output((SCROLLBACK_LIMIT + 2).to_string()))
    );
}
