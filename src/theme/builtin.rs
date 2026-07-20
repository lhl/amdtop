pub(super) const BUILTIN_THEMES: &[(&str, &str)] = &[
    ("adapta", include_str!("../../themes/adapta.toml")),
    ("adwaita", include_str!("../../themes/adwaita.toml")),
    (
        "adwaita-dark",
        include_str!("../../themes/adwaita-dark.toml"),
    ),
    ("ayu", include_str!("../../themes/ayu.toml")),
    ("dracula", include_str!("../../themes/dracula.toml")),
    ("dusklight", include_str!("../../themes/dusklight.toml")),
    (
        "elementarish",
        include_str!("../../themes/elementarish.toml"),
    ),
    (
        "everforest-dark-hard",
        include_str!("../../themes/everforest-dark-hard.toml"),
    ),
    (
        "everforest-dark-medium",
        include_str!("../../themes/everforest-dark-medium.toml"),
    ),
    (
        "everforest-light-medium",
        include_str!("../../themes/everforest-light-medium.toml"),
    ),
    ("flat-remix", include_str!("../../themes/flat-remix.toml")),
    (
        "flat-remix-light",
        include_str!("../../themes/flat-remix-light.toml"),
    ),
    (
        "flexoki-dark",
        include_str!("../../themes/flexoki-dark.toml"),
    ),
    (
        "flexoki-light",
        include_str!("../../themes/flexoki-light.toml"),
    ),
    ("gotham", include_str!("../../themes/gotham.toml")),
    ("greyscale", include_str!("../../themes/greyscale.toml")),
    (
        "gruvbox_dark",
        include_str!("../../themes/gruvbox_dark.toml"),
    ),
    (
        "gruvbox_dark_v2",
        include_str!("../../themes/gruvbox_dark_v2.toml"),
    ),
    (
        "gruvbox_light",
        include_str!("../../themes/gruvbox_light.toml"),
    ),
    (
        "gruvbox_material_dark",
        include_str!("../../themes/gruvbox_material_dark.toml"),
    ),
    ("horizon", include_str!("../../themes/horizon.toml")),
    (
        "HotPurpleTrafficLight",
        include_str!("../../themes/HotPurpleTrafficLight.toml"),
    ),
    (
        "kanagawa-dragon",
        include_str!("../../themes/kanagawa-dragon.toml"),
    ),
    (
        "kanagawa-lotus",
        include_str!("../../themes/kanagawa-lotus.toml"),
    ),
    (
        "kanagawa-wave",
        include_str!("../../themes/kanagawa-wave.toml"),
    ),
    ("kyli0x", include_str!("../../themes/kyli0x.toml")),
    (
        "matcha-dark-sea",
        include_str!("../../themes/matcha-dark-sea.toml"),
    ),
    ("monokai", include_str!("../../themes/monokai.toml")),
    ("night-owl", include_str!("../../themes/night-owl.toml")),
    ("nord", include_str!("../../themes/nord.toml")),
    ("onedark", include_str!("../../themes/onedark.toml")),
    ("orange", include_str!("../../themes/orange.toml")),
    ("paper", include_str!("../../themes/paper.toml")),
    (
        "phoenix-night",
        include_str!("../../themes/phoenix-night.toml"),
    ),
    (
        "solarized_dark",
        include_str!("../../themes/solarized_dark.toml"),
    ),
    (
        "solarized_light",
        include_str!("../../themes/solarized_light.toml"),
    ),
    ("tokyo-night", include_str!("../../themes/tokyo-night.toml")),
    ("tokyo-storm", include_str!("../../themes/tokyo-storm.toml")),
    (
        "tomorrow-night",
        include_str!("../../themes/tomorrow-night.toml"),
    ),
    ("twilight", include_str!("../../themes/twilight.toml")),
    ("whiteout", include_str!("../../themes/whiteout.toml")),
];

pub(super) fn get(name: &str) -> Option<&'static str> {
    BUILTIN_THEMES
        .iter()
        .find_map(|(candidate, text)| (*candidate == name).then_some(*text))
}

pub(super) fn names() -> impl Iterator<Item = &'static str> {
    BUILTIN_THEMES.iter().map(|(name, _)| *name)
}
