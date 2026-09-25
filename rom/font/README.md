# System font

Tamzen 5×9 regular is the provisional System ROM font,
copied unmodified from <https://github.com/sunaku/tamzen-font> at commit
`3255e8259bc9b880c60ab8b737ec8aa574e00d75` (`bdf/Tamzen5x9r.bdf`).

Tamzen is derived from Tamsyn. Both licenses are in `LICENSE-Tamzen`, copied
unmodified from the upstream repository.

Glaux only loads the printable ASCII range `0x20..=0x7E` from this file. Each
glyph is 5×9 pixels and is placed in a 6×10 character cell.
