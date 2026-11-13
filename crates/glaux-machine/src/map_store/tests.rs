use super::*;

fn layers(sizes: [(usize, usize); 3]) -> [TileLayer; 3] {
    sizes.map(|(width, height)| TileLayer::empty(width, height).unwrap())
}

#[test]
fn layers_of_one_size_form_a_map() {
    let map = MapData::new(layers([(5, 3); 3])).unwrap();
    assert_eq!((map.width(), map.height()), (5, 3));
    assert_eq!(map.layer(BgLayer::Bg2).cell_count(), 15);
}

#[test]
fn layers_of_different_sizes_are_refused() {
    assert!(MapData::new(layers([(5, 3), (5, 3), (3, 5)])).is_none());
}

#[test]
fn a_store_finds_maps_by_id_and_replaces_duplicates() {
    let mut store = MapStore::default();
    assert!(store.is_empty());
    store.insert("town", MapData::new(layers([(2, 2); 3])).unwrap());
    store.insert("town", MapData::new(layers([(4, 4); 3])).unwrap());
    assert_eq!(store.get("town").unwrap().width(), 4);
    assert!(store.get("cave").is_none());
}
