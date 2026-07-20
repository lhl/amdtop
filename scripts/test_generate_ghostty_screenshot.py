import unittest

from scripts.generate_ghostty_screenshot import (
    expanded_capture_state,
    resize_for_grid,
    select_window,
)


class GhosttyScreenshotTests(unittest.TestCase):
    def test_resize_scales_window_to_the_requested_grid(self):
        self.assertEqual(resize_for_grid(1518, 1674, 138, 68, 120, 48), (1320, 1182))
        self.assertEqual(resize_for_grid(1320, 1182, 120, 48, 120, 48), (1320, 1182))

    def test_expanded_state_requires_every_section_and_theme(self):
        state = {
            "cpu": False,
            "gpu": False,
            "npu": False,
            "processes": False,
            "theme": "tokyo-night",
        }
        self.assertTrue(expanded_capture_state(state, "tokyo-night"))
        state["gpu"] = True
        self.assertFalse(expanded_capture_state(state, "tokyo-night"))
        state["gpu"] = False
        self.assertFalse(expanded_capture_state(state, "onedark"))

    def test_window_selection_requires_the_unique_title(self):
        windows = [
            {"id": 1, "title": "other"},
            {"id": 2, "title": "amdtop-capture"},
        ]
        self.assertEqual(select_window(windows, "amdtop-capture"), windows[1])
        self.assertIsNone(select_window(windows, "missing"))


if __name__ == "__main__":
    unittest.main()
