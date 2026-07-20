import unittest

from scripts.gpu_load import duty_cycle


class GpuLoadTests(unittest.TestCase):
    def test_duty_cycle_repeats_the_requested_sweep(self):
        levels = (0.2, 0.45, 0.7, 1.0)
        self.assertEqual(duty_cycle(0.0, 2.0, levels), 0.2)
        self.assertEqual(duty_cycle(1.99, 2.0, levels), 0.2)
        self.assertEqual(duty_cycle(2.0, 2.0, levels), 0.45)
        self.assertEqual(duty_cycle(6.0, 2.0, levels), 1.0)
        self.assertEqual(duty_cycle(8.0, 2.0, levels), 0.2)

    def test_duty_cycle_rejects_invalid_inputs(self):
        with self.assertRaises(ValueError):
            duty_cycle(0.0, 0.0, (0.5,))
        with self.assertRaises(ValueError):
            duty_cycle(0.0, 1.0, ())
        with self.assertRaises(ValueError):
            duty_cycle(0.0, 1.0, (0.0,))
        with self.assertRaises(ValueError):
            duty_cycle(0.0, 1.0, (1.1,))


if __name__ == "__main__":
    unittest.main()
