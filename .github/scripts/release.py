"""プラグイン集のリリース用スクリプト（GitHub Actions から呼ぶ。手元でも dry run できる）。
リポジトリごとの名前などは .github/release.json に書く（JK-Plugins と fs-rs で同じスクリプトを使う）。

サブコマンド
  plan [--tag vYYYY.MM.DD[.N]] [--out DIR]
      リリースの計画を作る：まとめリリースのタグ、プラグインの一覧とバージョン、
      個別アーカイブを作るプラグイン、リリースノート。バージョンの上げ忘れがあれば失敗する。
  package-windows --plan DIR/plan.json --out DIR
      各プラグインをビルドして .aex と、まとめの zip・インストーラー（Inno Setup）を作る。
  package-macos --plan DIR/plan.json --out DIR
      各プラグインを Intel + Apple Silicon でビルドして .plugin を組み立て、
      ad-hoc 署名し、個別の .plugin.zip と、まとめの .pkg を作る。

方針（README の「リリース」も参照）
- プラグインのバージョンは各 Cargo.toml の version だけで管理する
- まとめリリースは日付のタグ（vYYYY.MM.DD、同じ日の2回目以降は .2, .3 …）
- バージョンが新しいプラグインだけ、<crate>-v<version> の個別アーカイブを作る
- 前回のアーカイブからプラグイン（とそれが使うワークスペース内の crate）が変わっているのに
  バージョンが同じなら、上げ忘れとして失敗する
"""
import argparse
import datetime
import json
import os
import plistlib
import re
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BUNDLE_TAG = re.compile(r"^v(\d{4})\.(\d{2})\.(\d{2})(?:\.(\d+))?$")
MEDIACORE_MAC = "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore"
# プラグインは MediaCore の中の JK-Plugins フォルダーにまとめて入れる（After Effects は中のフォルダーも読む）
PLUGINS_SUBDIR = "JK-Plugins"
PLUGINS_DIR_MAC = f"{MEDIACORE_MAC}/{PLUGINS_SUBDIR}"
# リポジトリごとの設定（.github/release.json）
#   repo：owner/name、name：表示名、file_prefix：ファイル名の先頭、
#   installer_app_id：Windows インストーラーの AppId（アップデート・アンインストールで同じアプリとして扱う。変えないこと）、
#   pkg_identifier：macOS pkg の ID、uninstall_dir：Program Files の中のアンインストーラーの置き場所
CONFIG = json.loads((ROOT / ".github" / "release.json").read_text(encoding="utf-8"))
REPO = CONFIG["repo"]
NAME = CONFIG["name"]
FILE_PREFIX = CONFIG["file_prefix"]
INSTALLER_APP_ID = CONFIG["installer_app_id"]
PKG_IDENTIFIER = CONFIG["pkg_identifier"]
UNINSTALL_DIR = CONFIG["uninstall_dir"]


def run(*args, cwd=ROOT, check=True, capture=True):
    r = subprocess.run(args, cwd=cwd, check=False, text=True, encoding="utf-8", errors="replace", capture_output=capture)
    if check and r.returncode != 0:
        raise SystemExit(f"command failed ({r.returncode}): {' '.join(map(str, args))}\n{r.stdout}\n{r.stderr}")
    return r.stdout if capture else ""


def git(*args, check=True):
    return run("git", *args, check=check).strip()


def tag_exists(tag):
    return bool(git("tag", "--list", tag))


# ---------------------------------------------------------------- プラグインの一覧


def plugins():
    """ワークスペースの中で [package.metadata.jk_plugin] を持つ crate"""
    meta = json.loads(run("cargo", "metadata", "--format-version", "1", "--no-deps"))
    members = set(meta["workspace_members"])
    by_dir = {}
    for pkg in meta["packages"]:
        by_dir[Path(pkg["manifest_path"]).parent.resolve()] = pkg["name"]
    out = []
    for pkg in meta["packages"]:
        if pkg["id"] not in members:
            continue
        jk = (pkg.get("metadata") or {}).get("jk_plugin")
        if not jk:
            continue
        crate_dir = Path(pkg["manifest_path"]).parent.resolve()
        # ワークスペース内の path 依存（libs など）も「このプラグインの中身」として扱う
        deps = [crate_dir]
        for d in pkg["dependencies"]:
            if d.get("path"):
                deps.append(Path(d["path"]).resolve())
        out.append(
            {
                "crate": pkg["name"],
                "lib": pkg["name"].replace("-", "_"),
                "version": pkg["version"],
                "plugin_name": jk["plugin_name"],
                "identifier": jk["identifier"],
                "dir": crate_dir.relative_to(ROOT).as_posix(),
                "watch": sorted({p.relative_to(ROOT).as_posix() for p in deps if ROOT in p.parents or p == ROOT}),
            }
        )
    out.sort(key=lambda p: p["crate"])
    return out


def asset_stem(plugin):
    """ダウンロード用のファイル名（GitHub は空白を . に変えるのでハイフンにする）"""
    return plugin["plugin_name"].replace(" ", "-")


# ---------------------------------------------------------------- plan


def next_bundle_tag(today=None):
    today = today or datetime.datetime.now(datetime.timezone(datetime.timedelta(hours=9))).date()
    base = f"v{today:%Y.%m.%d}"
    if not tag_exists(base):
        return base
    n = 2
    while tag_exists(f"{base}.{n}"):
        n += 1
    return f"{base}.{n}"


def bundle_tags():
    tags = [t for t in git("tag", "--list", "v*").splitlines() if BUNDLE_TAG.match(t)]

    def key(t):
        m = BUNDLE_TAG.match(t)
        return tuple(int(g or 1) for g in m.groups())

    return sorted(tags, key=key)


def version_at(ref, plugin):
    """ref の時点の Cargo.toml の version（無ければ None）"""
    r = subprocess.run(
        ["git", "show", f"{ref}:{plugin['dir']}/Cargo.toml"], cwd=ROOT, text=True, encoding="utf-8", errors="replace", capture_output=True
    )
    if r.returncode != 0:
        return None
    in_package = False
    for line in r.stdout.splitlines():
        s = line.strip()
        if s.startswith("["):
            in_package = s == "[package]"
        elif in_package:
            m = re.match(r'version\s*=\s*"([^"]+)"', s)
            if m:
                return m.group(1)
    return None


def changed_since(ref, paths):
    r = subprocess.run(["git", "diff", "--quiet", ref, "HEAD", "--", *paths], cwd=ROOT)
    return r.returncode != 0


def readme_version_errors(plist):
    """README のプラグイン一覧の行（| [名前](dir/README.md) | [版](…/tag/<crate>-v<版>) | 説明 |）が
    Cargo.toml の version と合っているか"""
    errors = []
    for readme in sorted(ROOT.glob("README*.md")):
        errors += _readme_version_errors(readme, plist)
    return errors


def _readme_version_errors(readme, plist):
    text = readme.read_text(encoding="utf-8")
    errors = []
    for p in plist:
        row = next((l for l in text.splitlines() if f"]({p['dir']}/README.md)" in l and l.startswith("|")), None)
        expected = f"[{p['version']}](https://github.com/{REPO}/releases/tag/{p['archive_tag']})"
        if row is None:
            errors.append(f"{p['crate']}: {readme.name} のプラグイン一覧に行がない（{expected} を書く）")
        elif expected not in row:
            errors.append(f"{p['crate']}: {readme.name} のプラグイン一覧のバージョンを {expected} にする")
    return errors


def cmd_plan(args):
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    tag = args.tag or next_bundle_tag()
    m = BUNDLE_TAG.match(tag)
    if not m:
        raise SystemExit(f"bundle tag must look like vYYYY.MM.DD or vYYYY.MM.DD.N: {tag}")
    bundle_version = tag[1:]
    previous = [t for t in bundle_tags() if t != tag]
    prev_tag = previous[-1] if previous else None

    plist = plugins()
    if not plist:
        raise SystemExit("no plug-ins with [package.metadata.jk_plugin] found")
    errors = []
    for p in plist:
        p["archive_tag"] = f"{p['crate']}-v{p['version']}"
        p["asset_stem"] = asset_stem(p)
        if tag_exists(p["archive_tag"]):
            p["new_version"] = False
            if changed_since(p["archive_tag"], p["watch"]):
                errors.append(
                    f"{p['crate']}: {', '.join(p['watch'])} changed since {p['archive_tag']} "
                    f"but version is still {p['version']}. Bump version in {p['dir']}/Cargo.toml."
                )
        else:
            p["new_version"] = True
        p["previous_version"] = version_at(prev_tag, p) if prev_tag else None
    errors += readme_version_errors(plist)
    if errors:
        raise SystemExit("version check failed:\n  " + "\n  ".join(errors))

    # リリースノート
    updated = [p for p in plist if p["previous_version"] != p["version"]]
    same = [p for p in plist if p["previous_version"] == p["version"]]
    lines = [f"# {NAME} {bundle_version}", ""]
    if updated:
        lines.append("## 更新")
        for p in updated:
            if p["previous_version"]:
                lines.append(f"- {p['plugin_name']} {p['previous_version']} → {p['version']}")
            else:
                lines.append(f"- {p['plugin_name']} {p['version']}（新規）")
        lines.append("")
    if same:
        lines.append("## 変更なし")
        for p in same:
            lines.append(f"- {p['plugin_name']} {p['version']}")
        lines.append("")
    lines += [
        "## インストール",
        "",
        f"- **Windows**: `{FILE_PREFIX}-{bundle_version}-windows-setup.exe` を実行する（After Effects は終了しておく）。"
        "署名していないので「Windows によって PC が保護されました」と出たら「詳細情報」→「実行」を押す。"
        f"アンインストールは 設定 → アプリ → インストールされているアプリ の {NAME} から。"
        f"インストーラーを使わないときは `{FILE_PREFIX}-{bundle_version}-windows.zip` を解凍し、中の `JK-Plugins` フォルダーを "
        "`C:\\Program Files\\Adobe\\Common\\Plug-ins\\7.0\\MediaCore\\` にコピーする",
        f"- **macOS**: `{FILE_PREFIX}-{bundle_version}-macos.pkg` を開く（MediaCore の中の JK-Plugins フォルダーに入る）。署名していないので最初は開けないため、"
        "システム設定 → プライバシーとセキュリティ の「このまま開く」を押してからもう一度開く",
        "",
        "プラグインごとのファイル（`.aex` / `.plugin.zip`）も同じリリースに置いてある。",
    ]
    if prev_tag:
        lines += ["", f"前回のまとめリリース：{prev_tag}"]
    notes = "\n".join(lines) + "\n"
    (out / "notes.md").write_text(notes, encoding="utf-8")

    plan = {
        "tag": tag,
        "bundle_version": bundle_version,
        "previous_tag": prev_tag,
        "sha": git("rev-parse", "HEAD"),
        "plugins": plist,
    }
    (out / "plan.json").write_text(json.dumps(plan, indent=2, ensure_ascii=False), encoding="utf-8")
    print(notes)
    print(json.dumps({k: v for k, v in plan.items() if k != "plugins"}, indent=2))
    for p in plist:
        print(f"{p['crate']} {p['version']} new_version={p['new_version']} watch={p['watch']}")
    gh_out = os.environ.get("GITHUB_OUTPUT")
    if gh_out:
        with open(gh_out, "a", encoding="utf-8") as f:
            f.write(f"tag={tag}\nbundle_version={bundle_version}\nname={NAME}\n")


# ---------------------------------------------------------------- Windows


def cmd_package_windows(args):
    plan = json.loads(Path(args.plan).read_text(encoding="utf-8"))
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    bundle = out / f"{FILE_PREFIX}-{plan['bundle_version']}-windows.zip"
    staged = out / "installer-files"
    if staged.exists():
        shutil.rmtree(staged)
    staged.mkdir()
    with zipfile.ZipFile(bundle, "w", zipfile.ZIP_DEFLATED) as z:
        for p in plan["plugins"]:
            run("cargo", "build", "--release", "-p", p["crate"], capture=False)
            dll = target / "release" / f"{p['lib']}.dll"
            # まとめの zip とインストーラーの中は AE に入れるときの名前（cargo jk と同じ）。
            # zip は JK-Plugins フォルダーごと MediaCore にコピーできるようにする
            z.write(dll, f"{PLUGINS_SUBDIR}/{p['plugin_name']}.aex")
            shutil.copy2(dll, staged / f"{p['plugin_name']}.aex")
            # 個別のファイル（GitHub は空白を . に変えるのでハイフンの名前）
            shutil.copy2(dll, out / f"{p['asset_stem']}.aex")
            print(f"built {p['plugin_name']}.aex")
    print(bundle)
    print(build_installer(plan, staged, out))


# Windows のインストール先（Inno Setup の定数で書く）
MEDIACORE_WIN = r"{commonpf64}\Adobe\Common\Plug-ins\7.0\MediaCore"
MEDIACORE_WIN_TEXT = r"C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore"
PLUGINS_DIR_WIN = MEDIACORE_WIN + "\\" + PLUGINS_SUBDIR
PLUGINS_DIR_WIN_TEXT = MEDIACORE_WIN_TEXT + "\\" + PLUGINS_SUBDIR


def find_iscc():
    """Inno Setup のコンパイラ（ISCC.exe）。環境変数 ISCC か、よくある場所から探す"""
    candidates = [os.environ.get("ISCC"), shutil.which("iscc")]
    for env in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"]:
        base = os.environ.get(env)
        if base:
            candidates.append(str(Path(base) / "Inno Setup 6" / "ISCC.exe"))
            candidates.append(str(Path(base) / "Programs" / "Inno Setup 6" / "ISCC.exe"))
    for c in candidates:
        if c and Path(c).exists():
            return c
    raise SystemExit("ISCC.exe (Inno Setup 6) not found. Install Inno Setup or set ISCC.")


def build_installer(plan, staged, out):
    """全プラグインを MediaCore の JK-Plugins フォルダーに入れるインストーラー（Inno Setup）。署名はしない。
    前の版が MediaCore の直下に入れたファイルは消す（同じプラグインが2つ読み込まれないように）。
    アンインストーラーは Program Files の UNINSTALL_DIR に置く（MediaCore に余計な exe を置かない）"""
    version = plan["bundle_version"]
    names = ", ".join(f"{p['plugin_name']} {p['version']}" for p in plan["plugins"])
    # Windows のファイルバージョンは数字4つまで（2026.11.22 → 2026.11.22.0）
    file_version = ".".join((version.split(".") + ["0"])[:4])
    lines = [
        "; release.py が作る。手で編集しない",
        "[Setup]",
        # AppId は { を2つ重ねて書く（Inno Setup の決まり）
        "AppId={{" + INSTALLER_APP_ID + "}",
        f"AppName={NAME}",
        f"AppVersion={version}",
        f"AppVerName={NAME} {version}",
        "AppPublisher=JK-Plugins",
        f"AppPublisherURL=https://github.com/{REPO}",
        f"AppSupportURL=https://github.com/{REPO}/releases",
        f"VersionInfoVersion={file_version}",
        f"VersionInfoDescription={NAME} for After Effects ({names})",
        "DefaultDirName={commonpf64}\\" + UNINSTALL_DIR,
        "DisableDirPage=yes",
        "DisableProgramGroupPage=yes",
        "PrivilegesRequired=admin",
        "ArchitecturesAllowed=x64compatible",
        "ArchitecturesInstallIn64BitMode=x64compatible",
        # After Effects が .aex を使っていたら知らせる
        "CloseApplications=yes",
        "RestartApplications=no",
        f"OutputDir={out.resolve()}",
        f"OutputBaseFilename={FILE_PREFIX}-{version}-windows-setup",
        "Compression=lzma2",
        "SolidCompression=yes",
        "WizardStyle=modern",
        f"UninstallDisplayName={NAME} (After Effects)",
        "",
        "[Languages]",
        r'Name: "ja"; MessagesFile: "compiler:Languages\Japanese.isl"',
        r'Name: "en"; MessagesFile: "compiler:Default.isl"',
        "",
        "[Messages]",
        f"ja.WelcomeLabel2=After Effects 用のプラグイン（{names}）を%n{PLUGINS_DIR_WIN_TEXT} にインストールします。"
        "%n%n続ける前に After Effects を終了してください。",
        f"en.WelcomeLabel2=This installs After Effects plug-ins ({names}) into%n{PLUGINS_DIR_WIN_TEXT}."
        "%n%nPlease quit After Effects before continuing.",
        "",
        # 前の版が MediaCore の直下に入れたもの
        "[InstallDelete]",
    ]
    for p in plan["plugins"]:
        lines.append(f'Type: files; Name: "{MEDIACORE_WIN}\\{p["plugin_name"]}.aex"')
    lines += ["", "[Files]"]
    for p in plan["plugins"]:
        src = (staged / f"{p['plugin_name']}.aex").resolve()
        lines.append(f'Source: "{src}"; DestDir: "{PLUGINS_DIR_WIN}"; Flags: ignoreversion')
    lines += ["", "[UninstallDelete]", f'Type: dirifempty; Name: "{PLUGINS_DIR_WIN}"']
    script = out / "installer.iss"
    # ISCC は BOM 付き UTF-8 を Unicode として読む
    script.write_text("\r\n".join(lines) + "\r\n", encoding="utf-8-sig")
    run(find_iscc(), "/Q", str(script), capture=False)
    exe = out / f"{FILE_PREFIX}-{version}-windows-setup.exe"
    if not exe.exists():
        raise SystemExit(f"installer not produced: {exe}")
    return exe


# ---------------------------------------------------------------- macOS


def find_rsrc(target, crate, triple):
    """build.rs（pipl）が作る PiPL のリソース。ふつうは target/<triple>/release/<crate>.rsrc"""
    expected = target / triple / "release" / f"{crate}.rsrc"
    if expected.exists():
        return expected
    found = sorted(target.rglob(f"{crate}.rsrc"), key=lambda f: f.stat().st_mtime, reverse=True)
    print(f"{expected} not found; candidates: {[str(f) for f in found]}")
    if not found:
        raise SystemExit(f"{crate}.rsrc not found under {target}")
    return found[0]


def build_bundle(p, target, out):
    """Intel + Apple Silicon のユニバーサルな .plugin を組み立てて ad-hoc 署名する"""
    triples = ["x86_64-apple-darwin", "aarch64-apple-darwin"]
    for t in triples:
        run("cargo", "build", "--release", "-p", p["crate"], "--target", t, capture=False)
    name = p["plugin_name"]
    bundle = out / f"{name}.plugin"
    if bundle.exists():
        shutil.rmtree(bundle)
    (bundle / "Contents" / "MacOS").mkdir(parents=True)
    (bundle / "Contents" / "Resources").mkdir(parents=True)
    run(
        "lipo",
        "-create",
        *[str(target / t / "release" / f"lib{p['lib']}.dylib") for t in triples],
        "-output",
        str(bundle / "Contents" / "MacOS" / name),
    )
    shutil.copy2(find_rsrc(target, p["crate"], triples[0]), bundle / "Contents" / "Resources" / f"{name}.rsrc")
    (bundle / "Contents" / "PkgInfo").write_text("eFKTFXTC", encoding="ascii")
    with open(bundle / "Contents" / "Info.plist", "wb") as f:
        plistlib.dump(
            {
                "CFBundlePackageType": "eFKT",
                "CFBundleSignature": "FXTC",
                "CFBundleIdentifier": p["identifier"],
                "CFBundleExecutable": name,
                "CFBundleName": name,
                "CFBundleShortVersionString": p["version"],
                "CFBundleVersion": p["version"],
            },
            f,
        )
    # Apple Silicon は署名が無いと読み込めないので ad-hoc 署名だけしておく
    run("codesign", "--force", "--deep", "--sign", "-", str(bundle))
    return bundle


def cmd_package_macos(args):
    plan = json.loads(Path(args.plan).read_text(encoding="utf-8"))
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    run("rustup", "target", "add", "x86_64-apple-darwin", "aarch64-apple-darwin", capture=False)
    root = out / "pkgroot"
    if root.exists():
        shutil.rmtree(root)
    root.mkdir()
    for p in plan["plugins"]:
        bundle = build_bundle(p, target, root)
        # 個別のファイル：中は AE に入れるときの名前の .plugin（ditto でバンドルの構造を保つ）
        run(
            "ditto",
            "-c",
            "-k",
            "--sequesterRsrc",
            "--keepParent",
            str(bundle),
            str(out / f"{p['asset_stem']}.plugin.zip"),
        )
        print(f"built {bundle.name}")

    # まとめの pkg：MediaCore/JK-Plugins に全部入れ、インストール後に ad-hoc 署名し直す（ntsc-rs と同じ）
    scripts = out / "scripts"
    if scripts.exists():
        shutil.rmtree(scripts)
    scripts.mkdir()
    names = " ".join(f'"{p["plugin_name"]}.plugin"' for p in plan["plugins"])
    post = scripts / "postinstall"
    post.write_text(
        "#!/bin/sh\n"
        "# 署名していないプラグインを Apple Silicon で読み込めるよう、インストール先で ad-hoc 署名する。\n"
        "# 前の版が MediaCore の直下に入れたものは、同じプラグインが2つ読み込まれないよう消す\n"
        f'DEST="${{2:-{PLUGINS_DIR_MAC}}}"\n'
        f'OLD="{MEDIACORE_MAC}"\n'
        f"for p in {names}; do\n"
        '  if [ -d "$DEST/$p" ]; then /usr/bin/codesign --force --deep --sign - "$DEST/$p"; fi\n'
        '  if [ -d "$OLD/$p" ] && [ "$OLD" != "$DEST" ]; then /bin/rm -rf "$OLD/$p"; fi\n'
        "done\n"
        "exit 0\n",
        encoding="utf-8",
    )
    post.chmod(0o755)
    comp = out / "component.plist"
    run("pkgbuild", "--analyze", "--root", str(root), str(comp))
    with open(comp, "rb") as f:
        components = plistlib.load(f)
    for c in components:
        # 別の場所にある同じ ID のバンドルを探してそこへ入れる動きを止める
        c["BundleIsRelocatable"] = False
    with open(comp, "wb") as f:
        plistlib.dump(components, f)
    pkg = out / f"{FILE_PREFIX}-{plan['bundle_version']}-macos.pkg"
    run(
        "pkgbuild",
        "--root",
        str(root),
        "--component-plist",
        str(comp),
        "--install-location",
        PLUGINS_DIR_MAC,
        "--scripts",
        str(scripts),
        "--identifier",
        PKG_IDENTIFIER,
        "--version",
        plan["bundle_version"],
        "--min-os-version",
        "11.0",
        str(pkg),
    )
    print(pkg)


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("plan")
    p.add_argument("--tag")
    p.add_argument("--out", default="dist")
    p.set_defaults(func=cmd_plan)
    for name, func in [("package-windows", cmd_package_windows), ("package-macos", cmd_package_macos)]:
        q = sub.add_parser(name)
        q.add_argument("--plan", required=True)
        q.add_argument("--out", default="dist")
        q.set_defaults(func=func)
    args = ap.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
