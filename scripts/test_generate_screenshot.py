import unittest

from scripts.generate_screenshot import (
    build_parser,
    canvas_size,
    choose_auto_cores,
    color_to_rgb,
    parse_cpu_cores,
    xterm_rgb,
)


class ScreenshotHelperTests(unittest.TestCase):
    def test_canonical_capture_uses_the_application_default_theme(self):
        args = build_parser().parse_args([])
        self.assertEqual(args.theme, "tokyo-night")
        self.assertEqual(args.default_fg, "cfc9c2")
        self.assertEqual(args.default_bg, "1a1b26")

    def test_xterm_color_conversion_covers_palette_sections(self):
        self.assertEqual(xterm_rgb(9), (255, 0, 0))
        self.assertEqual(xterm_rgb(16), (0, 0, 0))
        self.assertEqual(xterm_rgb(196), (255, 0, 0))
        self.assertEqual(xterm_rgb(232), (8, 8, 8))
        self.assertEqual(xterm_rgb(255), (238, 238, 238))

    def test_terminal_colors_accept_defaults_names_hex_and_indexes(self):
        self.assertEqual(color_to_rgb("default", "282c34"), (40, 44, 52))
        self.assertEqual(color_to_rgb("brightblue", "282c34"), (0, 0, 255))
        self.assertEqual(color_to_rgb("61afef", "282c34"), (97, 175, 239))
        self.assertEqual(color_to_rgb("196", "282c34"), (255, 0, 0))
        self.assertEqual(color_to_rgb("invalid", "282c34"), (40, 44, 52))

    def test_auto_cpu_selection_is_spread_across_allowed_cores(self):
        self.assertEqual(choose_auto_cores(range(32), 4), [0, 10, 21, 31])
        self.assertEqual(choose_auto_cores([2, 4], 4), [2, 4])
        self.assertEqual(choose_auto_cores(range(8), 0), [])

    def test_cpu_core_spec_validates_the_affinity_mask(self):
        self.assertEqual(parse_cpu_cores("auto", range(8), 3), [0, 4, 7])
        self.assertEqual(parse_cpu_cores("none", range(8), 4), [])
        self.assertEqual(parse_cpu_cores("5,1,5", range(8), 4), [5, 1])
        with self.assertRaisesRegex(ValueError, "outside"):
            parse_cpu_cores("8", range(8), 4)

    def test_canvas_size_includes_cell_grid_and_margin(self):
        self.assertEqual(canvas_size(188, 46, 10, 20, 4), (1888, 928))


if __name__ == "__main__":
    unittest.main()
