"""Download the pinned models/binaries listed in models.toml.

Usage: python -m sudo_core.models.fetch ../models.toml ../models [--llama-variant cuda-13.4]

Hugging Face entries are pinned to a commit `revision`. The llama.cpp release asset variant (CUDA build) is
chosen by --llama-variant, else $SUDO_LLAMA_VARIANT, else the manifest's `variant`; it must be one of
`variants`. The revision / tag / variant used is recorded in <dest>/lock.json.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import tomllib
import zipfile
from collections.abc import Mapping
from dataclasses import dataclass, field
from pathlib import Path
from typing import Literal

import httpx
from huggingface_hub import hf_hub_download, snapshot_download

Kind = Literal["hf_file", "hf_repo", "github_release"]
LLAMA_CPP_DIR = "llama.cpp"
VARIANT_ENV = "SUDO_LLAMA_VARIANT"
_COMMIT = re.compile(r"[0-9a-f]{40}")


@dataclass(frozen=True)
class ModelEntry:
    name: str
    kind: Kind
    repo: str
    files: list[str]
    pin: str | None = None  # github_release: release tag (None = latest)
    revision: str | None = None  # hf_file / hf_repo: 40-hex commit SHA
    variant: str | None = None  # github_release: default asset variant, fills "{variant}" in files
    variants: list[str] = field(default_factory=list)  # github_release: allowed variants


def load_manifest(path: Path) -> list[ModelEntry]:
    with open(path, "rb") as f:
        data = tomllib.load(f)
    entries = [
        ModelEntry(
            name=m["name"],
            kind=m["kind"],
            repo=m["repo"],
            files=list(m.get("files", [])),
            pin=m.get("pin"),
            revision=m.get("revision"),
            variant=m.get("variant"),
            variants=list(m.get("variants", [])),
        )
        for m in data["model"]
    ]
    for e in entries:
        if e.kind != "github_release" and not _COMMIT.fullmatch(e.revision or ""):
            raise ValueError(f"{e.name}: Hugging Face entries need revision = <40-hex commit SHA>, got {e.revision!r}")
    return entries


def resolve_variant(entry: ModelEntry, cli: str | None, env: Mapping[str, str] | None = None) -> str | None:
    """CLI flag > $SUDO_LLAMA_VARIANT > manifest default; must be listed in `variants` when that list is given."""
    env = os.environ if env is None else env
    variant = cli or env.get(VARIANT_ENV) or entry.variant
    if entry.variants and variant not in entry.variants:
        raise ValueError(f"{entry.name}: unknown variant {variant!r}; choose one of {entry.variants}")
    return variant


def asset_names(entry: ModelEntry, tag: str, variant: str | None) -> list[str]:
    return [t.format(tag=tag, variant=variant) for t in entry.files]


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(1 << 20):
            h.update(chunk)
    return h.hexdigest()


def _hash_tree(root: Path) -> dict[str, str]:
    """sha256 of every file under root, skipping the huggingface bookkeeping dir."""
    return {
        p.relative_to(root).as_posix(): _sha256(p)
        for p in sorted(root.rglob("*"))
        if p.is_file() and ".cache" not in p.relative_to(root).parts
    }


def _latest_release_tag(repo: str) -> str:
    resp = httpx.get(
        f"https://api.github.com/repos/{repo}/releases/latest",
        follow_redirects=True,
        timeout=30,
    )
    resp.raise_for_status()
    return resp.json()["tag_name"]


def _download(url: str, target: Path) -> None:
    with httpx.stream("GET", url, follow_redirects=True, timeout=60) as resp:
        resp.raise_for_status()
        with open(target, "wb") as f:
            for chunk in resp.iter_bytes(1 << 20):
                f.write(chunk)


def _fetch_github_release(entry: ModelEntry, out: Path, variant: str | None) -> dict:
    tag = entry.pin or _latest_release_tag(entry.repo)
    staging = out.with_name(out.name + ".download")
    shutil.rmtree(staging, ignore_errors=True)
    staging.mkdir(parents=True)
    hashes: dict[str, str] = {}
    archives: list[Path] = []
    for asset in asset_names(entry, tag, variant):
        url = f"https://github.com/{entry.repo}/releases/download/{tag}/{asset}"
        archive = staging / asset
        print(f"[{entry.name}] downloading {url}", flush=True)
        _download(url, archive)
        hashes[asset] = _sha256(archive)
        archives.append(archive)
    # Replace the old build only after every asset downloaded. A fresh directory means no stale DLLs
    # from another variant (e.g. CUDA 12 runtime DLLs next to a CUDA 13 ggml-cuda.dll).
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    for archive in archives:
        with zipfile.ZipFile(archive) as zf:
            zf.extractall(out)
    shutil.rmtree(staging, ignore_errors=True)
    return {"repo": entry.repo, "tag": tag, "variant": variant, "files": hashes}


def _fetch_one(entry: ModelEntry, out: Path, variant: str | None) -> dict:
    if entry.kind == "github_release":
        return _fetch_github_release(entry, out, resolve_variant(entry, variant))
    out.mkdir(parents=True, exist_ok=True)
    if entry.kind == "hf_file":
        for filename in entry.files:
            print(f"[{entry.name}] downloading {entry.repo}/{filename} @ {entry.revision}", flush=True)
            hf_hub_download(entry.repo, filename, revision=entry.revision, local_dir=out)
        hashes = {f: _sha256(out / f) for f in entry.files}
    elif entry.kind == "hf_repo":
        print(f"[{entry.name}] downloading {entry.repo} @ {entry.revision}", flush=True)
        snapshot_download(entry.repo, revision=entry.revision, local_dir=out)
        hashes = _hash_tree(out)
    else:
        raise ValueError(f"unknown kind {entry.kind!r} for {entry.name}")
    return {"repo": entry.repo, "revision": entry.revision, "files": hashes}


def fetch_all(manifest: Path, dest: Path, variant: str | None = None) -> dict[str, Path]:
    """Fetch every manifest entry into dest; returns name -> local directory.

    `variant` overrides the llama.cpp asset variant (see resolve_variant). Writes dest/lock.json with the
    sha256 of every fetched file plus the HF revision, or the release tag and variant, that was used.
    """
    dest.mkdir(parents=True, exist_ok=True)
    entries = load_manifest(manifest)
    for entry in entries:  # reject a bad variant before downloading gigabytes
        if entry.kind == "github_release":
            resolve_variant(entry, variant)
    paths: dict[str, Path] = {}
    lock: dict[str, dict] = {}
    for entry in entries:
        out = dest / (LLAMA_CPP_DIR if entry.kind == "github_release" else entry.name)
        lock[entry.name] = _fetch_one(entry, out, variant)
        paths[entry.name] = out
    (dest / "lock.json").write_text(json.dumps(lock, indent=2), encoding="utf-8")
    return paths


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(prog="python -m sudo_core.models.fetch")
    ap.add_argument("manifest", type=Path)
    ap.add_argument("dest", type=Path)
    ap.add_argument("--llama-variant", default=None,
                    help=f"llama.cpp asset variant, e.g. cuda-13.4 for an RTX 50xx (default: ${VARIANT_ENV}, "
                         "else models.toml)")
    args = ap.parse_args(argv)
    for name, path in fetch_all(args.manifest, args.dest, args.llama_variant).items():
        print(f"{name}: {path}")


if __name__ == "__main__":
    main()
