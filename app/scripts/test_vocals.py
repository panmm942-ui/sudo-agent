"""Optional audio runtime tests; synthetic samples only, no devices or route changes."""
import importlib.util
import io
import pathlib
import queue
import sys
import tempfile
import threading
import types
import unittest
from unittest.mock import patch
import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).parents[1] / 'src-tauri' / 'resources'))
from vocal_engine import Pipeline, BLOCK, FRESH, device_list, routing_lock
from vocal_route import Routes, restore


class VocalTests(unittest.TestCase):
    def test_only_one_process_can_own_spotify_routing(self):
        from filelock import Timeout
        with tempfile.TemporaryDirectory() as folder:
            with routing_lock(folder):
                with self.assertRaises(Timeout):
                    with routing_lock(folder): pass
            with routing_lock(folder): pass

    def test_blend_endpoints_and_silence(self):
        original = np.full((BLOCK, 2), .6, dtype=np.float32)
        instrumental = np.full_like(original, .2)
        for volume, expected in [(0, .2), (1, .6), (.5, .4)]:
            pipeline = Pipeline(None, volume)
            pipeline.outgoing.put((instrumental, original))
            output = np.zeros_like(original)
            pipeline.callback(original, output, BLOCK, None, None)
            np.testing.assert_allclose(output, expected)
        pipeline = Pipeline(None, 0)
        pipeline.stop.set()
        pipeline.callback(original, output, BLOCK, None, None)
        self.assertFalse(output.any())

    def test_overload_stops_instead_of_accumulating_delay(self):
        pipeline = Pipeline(None, 0)
        source = np.zeros((BLOCK, 2), dtype=np.float32)
        for _ in range(FRESH * 3 + 1):
            pipeline.callback(source, source.copy(), BLOCK, None, None)
        self.assertTrue(pipeline.stop.is_set())
        self.assertIn('fell behind', pipeline.error)

    def test_parent_pipe_closure_stops_processing(self):
        pipeline = Pipeline(None, .3)
        with patch('sys.stdin', io.StringIO('{"command":"volume","value":80}\n')):
            pipeline.controls()
        self.assertTrue(pipeline.stop.is_set())
        self.assertEqual(pipeline.volume, .8)

    def test_volume_changes_are_ramped_inside_each_audio_block(self):
        pipeline = Pipeline(None, 0)
        pipeline.volume = 1
        instrumental = np.zeros((BLOCK, 2), dtype=np.float32)
        original = np.ones_like(instrumental)
        pipeline.outgoing.put((instrumental, original))
        output = np.zeros_like(original)
        pipeline.callback(original, output, BLOCK, None, None)
        self.assertEqual(output[0, 0], 0)
        self.assertAlmostEqual(float(output[-1, 0]), .65, places=5)
        self.assertLess(float(np.max(np.abs(np.diff(output[:, 0])))), .001)

    def test_worker_keeps_continuous_sample_order(self):
        class Model:
            def demix(self, source): return source.copy()
        pipeline = Pipeline(Model(), 0)
        thread = threading.Thread(target=pipeline.worker, daemon=True)
        thread.start()
        try:
            for value in range(1, FRESH * 2 + 1):
                pipeline.incoming.put(np.full((BLOCK, 2), value, dtype=np.float32))
            result = [pipeline.outgoing.get(timeout=3)[1][0, 0] for _ in range(FRESH * 2)]
            self.assertEqual(result, list(range(FRESH * 2)))
        finally:
            pipeline.stop.set()
            thread.join(timeout=2)

    def test_only_cable_input_and_physical_wasapi_outputs(self):
        def device(name, inputs=0, outputs=0, hostapi=0):
            return dict(name=name, max_input_channels=inputs, max_output_channels=outputs, hostapi=hostapi)
        fake = types.SimpleNamespace(query_devices=lambda: [device('Microphone', 2), device('CABLE Output', 2),
            device('CABLE Input', outputs=2), device('Speakers', outputs=2), device('Other API speakers', outputs=2, hostapi=1)],
            query_hostapis=lambda i: {'name': 'Windows WASAPI' if i == 0 else 'MME'})
        with patch.dict(sys.modules, {'sounddevice': fake}):
            self.assertEqual([(d['name'], d['kind']) for d in device_list()], [('CABLE Output', 'input'), ('Speakers', 'output')])

    def test_route_saved_before_mutation_and_restored(self):
        with tempfile.TemporaryDirectory() as folder:
            backup = pathlib.Path(folder) / 'routes.json'
            calls = []
            class Policy:
                def get(self, pid, role): return 'original' if role == 0 else None
                def set(self, pid, role, endpoint):
                    self_test.assertTrue(backup.exists())
                    calls.append((pid, role, endpoint))
                def close(self): pass
            self_test = self
            with patch('vocal_route.Policy', Policy), patch('vocal_route.spotify_processes', return_value=[(101, 2.5)]):
                routes = Routes(backup, '{endpoint}')
                routes.update()
                self.assertEqual(calls[0][2], r'\\?\SWD#MMDEVAPI#{endpoint}#{e6327cad-dcec-4949-ae8a-991e976a79d2}')
                fake_process = types.SimpleNamespace(name=lambda: 'Spotify.exe', create_time=lambda: 2.5)
                with patch('psutil.Process', return_value=fake_process):
                    routes.close()
                self.assertFalse(backup.exists())
                self.assertEqual(calls[-2:], [(101, 0, 'original'), (101, 1, None)])

    def test_restoration_never_routes_reused_pid(self):
        with tempfile.TemporaryDirectory() as folder:
            backup = pathlib.Path(folder) / 'routes.json'
            backup.write_text('[{"pid":101,"created":2.5,"routes":{"0":null,"1":null}}]', encoding='utf-8')
            with patch('vocal_route.Policy') as policy, patch('psutil.Process') as process:
                process.return_value.name.return_value = 'different.exe'
                restore(backup)
                policy.return_value.set.assert_not_called()
                self.assertFalse(backup.exists())


if __name__ == '__main__': unittest.main()
