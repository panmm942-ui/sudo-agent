import io
import json
import re
import zipfile
from pathlib import Path

import pytest

from sudo_core.models import fetch
from sudo_core.models.fetch import ModelEntry, asset_names, load_manifest, resolve_variant

REPO = Path(__file__).resolve().parents[2]
REV = "e87f176479d0855a907a41277aca2f8ee7a09523"

MANIFEST = f"""
[[model]]
name = "llm"
kind = "hf_file"
repo = "unsloth/Qwen3.5-4B-GGUF"
files = ["Qwen3.5-4B-Q4_K_M.gguf", "mmproj-F16.gguf"]
revision = "{REV}"

[[model]]
name = "llama.cpp"
kind = "github_release"
repo = "ggml-org/llama.cpp"
files = ["llama-{{tag}}-bin-win-{{variant}}-x64.zip", "cudart-llama-bin-win-{{variant}}-x64.zip"]
pin = "b11982"
variant = "cuda-12.4"
variants = ["cuda-12.4", "cuda-13.4"]
"""


def _write(tmp_path: Path, text: str = MANIFEST) -> Path:
    path = tmp_path / "models.toml"
    path.write_text(text, encoding="utf-8")
    return path


def test_load_manifest_reads_all_entries(tmp_path: Path):
    entries = load_manifest(_write(tmp_path))

    assert entries == [
        ModelEntry(
            name="llm",
            kind="hf_file",
            repo="unsloth/Qwen3.5-4B-GGUF",
            files=["Qwen3.5-4B-Q4_K_M.gguf", "mmproj-F16.gguf"],
            revision=REV,
        ),
        ModelEntry(
            name="llama.cpp",
            kind="github_release",
            repo="ggml-org/llama.cpp",
            files=[
                "llama-{tag}-bin-win-{variant}-x64.zip",
                "cudart-llama-bin-win-{variant}-x64.zip",
            ],
            pin="b11982",
            variant="cuda-12.4",
            variants=["cuda-12.4", "cuda-13.4"],
        ),
    ]


def test_hf_entry_without_commit_revision_is_rejected(tmp_path: Path):
    text = MANIFEST.replace(f'revision = "{REV}"', 'revision = "main"')
    with pytest.raises(ValueError, match="revision"):
        load_manifest(_write(tmp_path, text))


def test_variant_precedence_cli_over_env_over_manifest(tmp_path: Path):
    llama = load_manifest(_write(tmp_path))[1]
    assert resolve_variant(llama, None, {}) == "cuda-12.4"
    assert resolve_variant(llama, None, {"SUDO_LLAMA_VARIANT": "cuda-13.4"}) == "cuda-13.4"
    assert resolve_variant(llama, "cuda-12.4", {"SUDO_LLAMA_VARIANT": "cuda-13.4"}) == "cuda-12.4"


def test_unknown_variant_is_rejected(tmp_path: Path):
    llama = load_manifest(_write(tmp_path))[1]
    with pytest.raises(ValueError, match="cuda-99"):
        resolve_variant(llama, "cuda-99", {})


def test_asset_names_fill_tag_and_variant(tmp_path: Path):
    llama = load_manifest(_write(tmp_path))[1]
    assert asset_names(llama, "b11982", "cuda-13.4") == [
        "llama-b11982-bin-win-cuda-13.4-x64.zip",
        "cudart-llama-bin-win-cuda-13.4-x64.zip",
    ]


def test_repo_manifest_pins_every_hf_entry_and_offers_a_blackwell_build():
    entries = load_manifest(REPO / "models.toml")
    hf = [e for e in entries if e.kind != "github_release"]
    assert hf and all(re.fullmatch(r"[0-9a-f]{40}", e.revision or "") for e in hf)
    llama = next(e for e in entries if e.kind == "github_release")
    assert llama.pin and llama.variant == "cuda-12.4"  # this dev box (GTX 1660 Super)
    assert any(v.startswith("cuda-13") for v in llama.variants)  # sm_120 (Blackwell) capable


def _zip_bytes(name: str) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        zf.writestr(name, b"binary")
    return buf.getvalue()


def test_fetch_all_uses_revision_and_variant_and_records_them(tmp_path: Path, monkeypatch):
    seen: dict = {"hf": [], "urls": []}

    def fake_hf(repo, filename, revision=None, local_dir=None):
        seen["hf"].append((repo, filename, revision))
        (Path(local_dir) / filename).write_bytes(b"x")

    def fake_download(url, target):
        seen["urls"].append(url)
        Path(target).write_bytes(_zip_bytes(url.rsplit("/", 1)[1] + ".dll"))

    monkeypatch.setattr(fetch, "hf_hub_download", fake_hf)
    monkeypatch.setattr(fetch, "_download", fake_download)
    dest = tmp_path / "models"
    (dest / "llama.cpp").mkdir(parents=True)
    (dest / "llama.cpp" / "stale-from-old-variant.dll").write_bytes(b"old")

    fetch.fetch_all(_write(tmp_path), dest, variant="cuda-13.4")

    assert {r for _, _, r in seen["hf"]} == {REV}
    assert all("cuda-13.4" in u and "/download/b11982/" in u for u in seen["urls"]) and len(seen["urls"]) == 2
    assert not (dest / "llama.cpp" / "stale-from-old-variant.dll").exists()
    lock = json.loads((dest / "lock.json").read_text(encoding="utf-8"))
    assert lock["llm"]["revision"] == REV
    assert lock["llama.cpp"]["variant"] == "cuda-13.4" and lock["llama.cpp"]["tag"] == "b11982"
