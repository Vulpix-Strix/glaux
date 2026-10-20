use pretty_assertions::assert_eq;

use super::*;

fn catalog(entries: &[(&str, Category, &str)]) -> Catalog {
    let entries = entries
        .iter()
        .map(|(name, category, license)| {
            serde_json::json!({
                "name": name,
                "version": "1.0.0",
                "license": license,
                "repository": null,
                "category": match category {
                    Category::Library => "library",
                    Category::Native => "native",
                    Category::Art => "art",
                },
                "text_ids": [],
            })
        })
        .collect::<Vec<_>>();
    let licenses = serde_json::from_value(serde_json::json!({
        "texts": [],
        "entries": entries,
    }))
    .expect("the fixture matches the inventory format");
    Catalog::new(licenses)
}

fn summary(catalog: &Catalog) -> Vec<(&str, usize, usize)> {
    catalog
        .summary()
        .iter()
        .map(|license| (license.id.as_str(), license.required, license.offered))
        .collect()
}

#[test]
fn the_summary_counts_required_and_offered_licenses_apart() {
    let catalog = catalog(&[
        ("a", Category::Library, "MIT"),
        ("b", Category::Library, "MIT OR Apache-2.0"),
        ("c", Category::Library, "Apache-2.0 OR MIT"),
        ("d", Category::Native, "Zlib"),
    ]);
    assert_eq!(
        summary(&catalog),
        [("MIT", 1, 2), ("Apache-2.0", 0, 2), ("Zlib", 1, 0)]
    );
}

#[test]
fn entries_without_a_license_are_counted_apart_from_the_summary() {
    let catalog = catalog(&[
        ("a", Category::Library, "MIT"),
        ("palette", Category::Art, ""),
    ]);
    assert_eq!(catalog.undeclared(), 1);
    assert_eq!(summary(&catalog), [("MIT", 1, 0)]);
    assert_eq!(catalog.terms(1), None);
}

#[test]
fn a_filter_keeps_components_that_mention_the_license_in_any_role() {
    let catalog = catalog(&[
        ("a", Category::Library, "MIT"),
        ("b", Category::Library, "Apache-2.0 OR MIT"),
        ("c", Category::Library, "Zlib"),
        ("d", Category::Art, ""),
    ]);
    let mit = LicenseFilter::Id("MIT".to_owned());
    let kept: Vec<_> = (0..4).filter(|&i| catalog.matches(i, &mit)).collect();
    assert_eq!(kept, [0, 1]);
    let kept: Vec<_> = (0..4)
        .filter(|&i| catalog.matches(i, &LicenseFilter::Undeclared))
        .collect();
    assert_eq!(kept, [3]);
}

#[test]
fn counts_follow_the_inventory() {
    let catalog = catalog(&[
        ("a", Category::Library, "MIT"),
        ("b", Category::Library, "MIT"),
        ("c", Category::Art, "CC0-1.0"),
    ]);
    assert_eq!(catalog.count(None), 3);
    assert_eq!(catalog.count(Some(Category::Library)), 2);
    assert_eq!(catalog.count(Some(Category::Native)), 0);
}
