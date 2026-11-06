"""Per-Spotify endpoint routing. Never changes Windows' global default device.

WinRT interface layout documented by EarTrumpet (MIT):
https://github.com/File-New-Project/EarTrumpet/tree/master/EarTrumpet/Interop/MMDeviceAPI
Only its public method signatures/slot layout are used; implementation is independent.
"""
import ctypes as c
import json
import os
import uuid
from pathlib import Path


class RouteError(RuntimeError):
    def __init__(self, code):
        super().__init__(f'Could not change Spotify audio route ({code:#x}).')
        self.code = code


class Policy:
    def __init__(self):
        self.dll = c.WinDLL('combase')
        self.dll.WindowsCreateString.argtypes = [c.c_wchar_p, c.c_uint32, c.POINTER(c.c_void_p)]
        self.dll.WindowsDeleteString.argtypes = [c.c_void_p]
        self.dll.WindowsGetStringRawBuffer.argtypes = [c.c_void_p, c.POINTER(c.c_uint32)]
        self.dll.WindowsGetStringRawBuffer.restype = c.c_wchar_p
        self.dll.RoGetActivationFactory.argtypes = [c.c_void_p, c.c_void_p, c.POINTER(c.c_void_p)]
        self.initialized = self.dll.RoInitialize(1) in (0, 1)
        name = self.string('Windows.Media.Internal.AudioPolicyConfig')
        self.ptr = c.c_void_p()
        try:
            for identifier in ['ab3d4648-e242-459f-b02f-541c70306324', '2a59116d-6c4f-45e0-a74f-707e3fef9258']:
                iid = c.create_string_buffer(uuid.UUID(identifier).bytes_le)
                hr = self.dll.RoGetActivationFactory(name, iid, c.byref(self.ptr))
                if hr == 0:
                    break
            else:
                raise RuntimeError('Windows per-app audio routing is unavailable.')
        finally:
            self.dll.WindowsDeleteString(name)

    def string(self, text):
        value = c.c_void_p()
        if self.dll.WindowsCreateString(text, len(text), c.byref(value)) != 0:
            raise RuntimeError('Could not prepare audio endpoint identifier.')
        return value

    def method(self, slot, *args):
        table = c.cast(self.ptr, c.POINTER(c.POINTER(c.c_void_p))).contents
        return c.WINFUNCTYPE(c.c_long, c.c_void_p, *args)(table[slot])

    def get(self, pid, role):
        value = c.c_void_p()
        hr = self.method(26, c.c_uint32, c.c_int, c.c_int, c.POINTER(c.c_void_p))(self.ptr, pid, 0, role, c.byref(value))
        if hr & 0xffffffff in (0x80070057, 0x80070490):
            return None  # No persisted app override on this Windows build.
        if hr < 0:
            raise RuntimeError(f'Could not read Spotify audio route ({hr:#x}).')
        try:
            return self.dll.WindowsGetStringRawBuffer(value, None) if value.value else None
        finally:
            if value.value:
                self.dll.WindowsDeleteString(value)

    def set(self, pid, role, endpoint):
        value = self.string(endpoint) if endpoint else c.c_void_p()
        try:
            hr = self.method(25, c.c_uint32, c.c_int, c.c_int, c.c_void_p)(self.ptr, pid, 0, role, value)
            if hr < 0:
                raise RouteError(hr & 0xffffffff)
        finally:
            if value.value:
                self.dll.WindowsDeleteString(value)

    def close(self):
        if self.ptr:
            self.method(2)(self.ptr)
            self.ptr = None
            if self.initialized:
                self.dll.RoUninitialize()


def spotify_processes():
    """Spotify's main process: the one that owns its audio session.

    Spotify is Chromium-based; its renderer/GPU/utility helpers cannot be routed
    (Windows answers E_INVALIDARG), and trying them used to abort vocal mode.
    """
    import psutil
    for process in psutil.process_iter(['name', 'create_time']):
        if (process.info['name'] or '').lower() != 'spotify.exe':
            continue
        try:
            if any(arg.startswith('--type=') for arg in process.cmdline()):
                continue
        except (psutil.AccessDenied, psutil.NoSuchProcess):
            continue
        yield process.pid, process.info['create_time']


INVALID_ARG = 0x80070057


def restore(path):
    import psutil
    path = Path(path)
    if not path.exists():
        return
    records = json.loads(path.read_text(encoding='utf-8'))
    policy = Policy()
    failures = []
    try:
        for record in records:
            try:
                process = psutil.Process(record['pid'])
                if process.name().lower() != 'spotify.exe' or abs(process.create_time() - record['created']) > .01:
                    continue
                for role in (0, 1):
                    try:
                        policy.set(record['pid'], role, record['routes'][str(role)])
                    except RouteError as error:
                        # Nothing to undo if Windows already reports the original route.
                        if error.code != INVALID_ARG or policy.get(record['pid'], role) != record['routes'][str(role)]:
                            raise
            except psutil.NoSuchProcess:
                continue
            except Exception:
                failures.append(record)
        if failures:
            path.write_text(json.dumps(failures), encoding='utf-8')
            raise RuntimeError('Some Spotify routes could not be restored. Use Windows Volume mixer → Spotify → Output device.')
        path.unlink(missing_ok=True)
    finally:
        policy.close()


class Routes:
    def __init__(self, backup, endpoint):
        self.backup = Path(backup)
        self.endpoint = '\\\\?\\SWD#MMDEVAPI#' + endpoint + '#{e6327cad-dcec-4949-ae8a-991e976a79d2}'
        self.records = []
        self.policy = Policy()

    def update(self):
        for pid, created in spotify_processes():
            if any(r['pid'] == pid and r['created'] == created for r in self.records):
                continue
            try:
                record = {'pid': pid, 'created': created, 'routes': {str(role): self.policy.get(pid, role) for role in (0, 1)}}
            except RuntimeError:
                continue
            self.records.append(record)
            self.backup.parent.mkdir(parents=True, exist_ok=True)
            pending = self.backup.with_suffix('.tmp')
            pending.write_text(json.dumps(self.records), encoding='utf-8')
            os.replace(pending, self.backup)
            try:
                for role in (0, 1):
                    self.policy.set(pid, role, self.endpoint)
            except RouteError:
                # Not routable (yet); forget it so the next update retries it.
                self.records.remove(record)
                for role in (0, 1):
                    try:
                        self.policy.set(pid, role, record['routes'][str(role)])
                    except RouteError:
                        pass
                pending.write_text(json.dumps(self.records), encoding='utf-8')
                os.replace(pending, self.backup)

    @property
    def routed(self):
        return bool(self.records)

    def close(self):
        self.policy.close()
        restore(self.backup)


def cable_endpoint():
    from pycaw.pycaw import AudioUtilities
    matches = [d for d in AudioUtilities.GetAllDevices() if 'CABLE' in d.FriendlyName.upper() and 'INPUT' in d.FriendlyName.upper() and d.id.startswith('{0.0.0.')]
    if len(matches) != 1:
        raise RuntimeError('Install one VB-Audio CABLE device pair before enabling vocal mode.')
    return matches[0].id
