use super::*;

#[test]
fn embedded_licenses_are_readable_and_complete() {
    let licenses = third_party().expect("the embedded license data parses");

    for category in [Category::Library, Category::Native, Category::Art] {
        assert!(
            licenses
                .entries
                .iter()
                .any(|entry| entry.category == category),
            "no {category:?} entries"
        );
    }
    for entry in &licenses.entries {
        assert!(
            licenses.texts_of(entry).next().is_some(),
            "{} has no license text",
            entry.name
        );
    }
    for text in &licenses.texts {
        assert!(!text.title.is_empty(), "a text has no title");
        assert!(!text.content.trim().is_empty(), "{} is empty", text.title);
    }
}
