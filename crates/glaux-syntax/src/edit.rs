use tree_sitter::{InputEdit, Point};

/// Describes the change from `old` to `new` as a single edit.
///
/// The edit spans everything between the common prefix and the common suffix.
/// One covering edit is valid for Tree-sitter no matter how many edits actually
/// happened, which keeps incremental parsing correct across undo, redo, and
/// reloads without tracking each change.
pub(crate) fn between(old: &str, new: &str) -> InputEdit {
    let (old_bytes, new_bytes) = (old.as_bytes(), new.as_bytes());

    let mut prefix = old_bytes
        .iter()
        .zip(new_bytes)
        .take_while(|(a, b)| a == b)
        .count();
    while !(old.is_char_boundary(prefix) && new.is_char_boundary(prefix)) {
        prefix -= 1;
    }

    let room = old.len().min(new.len()) - prefix;
    let mut suffix = old_bytes
        .iter()
        .rev()
        .zip(new_bytes.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    while suffix > 0
        && !(old.is_char_boundary(old.len() - suffix) && new.is_char_boundary(new.len() - suffix))
    {
        suffix -= 1;
    }

    let old_end_byte = old.len() - suffix;
    let new_end_byte = new.len() - suffix;
    let start_position = point_at(old, prefix);
    InputEdit {
        start_byte: prefix,
        old_end_byte,
        new_end_byte,
        start_position,
        old_end_position: advance(start_position, &old[prefix..old_end_byte]),
        new_end_position: advance(start_position, &new[prefix..new_end_byte]),
    }
}

/// The row and byte column of `byte` in `text`.
fn point_at(text: &str, byte: usize) -> Point {
    advance(Point { row: 0, column: 0 }, &text[..byte])
}

/// Moves `from` forward over `text`.
fn advance(from: Point, text: &str) -> Point {
    let bytes = text.as_bytes();
    match bytes.iter().rposition(|&b| b == b'\n') {
        None => Point {
            row: from.row,
            column: from.column + bytes.len(),
        },
        Some(last) => Point {
            row: from.row + bytes.iter().filter(|&&b| b == b'\n').count(),
            column: bytes.len() - last - 1,
        },
    }
}

#[cfg(test)]
mod tests;
