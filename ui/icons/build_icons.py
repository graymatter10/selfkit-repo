#!/usr/bin/env python3
"""Selfkit icon generator.

Each app glyph is defined once (1024x1024 artboard, kept inside the Android
adaptive-icon safe zone) and rendered into:
  ui/icons/svg/<app>.svg              full icon (squircle background + glyph)
  ui/icons/svg/<app>-foreground.svg   glyph only (Android adaptive foreground)
  ui/icons/svg/<app>-monochrome.svg   single-colour glyph (Android 13 themed icons, tray)
and rasterised (rsvg-convert + ImageMagick) into apps/<app>/icons/:
  32x32.png 64x64.png 128x128.png 128x128@2x.png icon.png(512) icon.ico
  android/mipmap-*/ic_launcher.png, ic_launcher_round.png, ic_launcher_foreground.png
plus ui/icons/preview.png (contact sheet).

Run from the repo root:  python3 ui/icons/build_icons.py
Docs: docs/design-system.md (Icons).
"""

import pathlib
import shutil
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
SVG_DIR = ROOT / "ui/icons/svg"

# Calm, muted palette: one accent per app (top-left light -> bottom-right deep).
APPS = {
    "chrono": ("#4b9a96", "#2d6865"),
    "hisab": ("#67a06b", "#3f6d45"),
    "tickd": ("#5d7fbd", "#384f86"),
    "reflect": ("#cf9a4e", "#9a6827"),
    "dusk": ("#7a69ad", "#463a73"),
    "agent": ("#5c636c", "#30353b"),
}

W = "#ffffff"


def glyph(app: str, fg: str, bg_deep: str) -> str:
    """SVG elements for an app glyph. `fg` = glyph colour, `bg_deep` = cut-out colour."""
    if app == "chrono":
        # Time ring: faint full track + bold 3/4 arc, centre hub and hand.
        return f"""
  <circle cx="512" cy="512" r="226" fill="none" stroke="{fg}" stroke-opacity=".28" stroke-width="58"/>
  <path d="M512 286 A226 226 0 1 1 286 512" fill="none" stroke="{fg}" stroke-width="58" stroke-linecap="round"/>
  <path d="M512 512 L512 382" stroke="{fg}" stroke-width="44" stroke-linecap="round"/>
  <path d="M512 512 L598 566" stroke="{fg}" stroke-width="44" stroke-linecap="round"/>
  <circle cx="512" cy="512" r="40" fill="{fg}"/>"""
    if app == "hisab":
        # Wallet: outlined body with a solid clasp and a coin cut-out.
        return f"""
  <path d="M330 372 L640 312 Q668 307 672 336 L676 362" fill="none" stroke="{fg}" stroke-opacity=".55" stroke-width="40" stroke-linecap="round" stroke-linejoin="round"/>
  <rect x="290" y="364" width="444" height="330" rx="64" fill="none" stroke="{fg}" stroke-width="50"/>
  <rect x="566" y="468" width="190" height="122" rx="61" fill="{fg}"/>
  <circle cx="627" cy="529" r="24" fill="{bg_deep}"/>"""
    if app == "tickd":
        # Checkbox with a confident tick that breaks out of the frame.
        return f"""
  <rect x="300" y="300" width="424" height="424" rx="104" fill="none" stroke="{fg}" stroke-opacity=".32" stroke-width="50"/>
  <path d="M392 516 L478 602 L676 386" fill="none" stroke="{fg}" stroke-width="68" stroke-linecap="round" stroke-linejoin="round"/>"""
    if app == "reflect":
        # Rising bars over a baseline, with a reflection dot.
        return f"""
  <rect x="318" y="520" width="92" height="182" rx="46" fill="{fg}" fill-opacity=".55"/>
  <rect x="466" y="414" width="92" height="288" rx="46" fill="{fg}" fill-opacity=".78"/>
  <rect x="614" y="310" width="92" height="392" rx="46" fill="{fg}"/>
  <path d="M300 760 L724 760" stroke="{fg}" stroke-opacity=".4" stroke-width="34" stroke-linecap="round"/>"""
    if app == "dusk":
        # Crescent moon, a small star, and a soft horizon.
        return f"""
  <defs><mask id="moon"><rect width="1024" height="1024" fill="#fff"/><circle cx="588" cy="392" r="178" fill="#000"/></mask></defs>
  <circle cx="478" cy="478" r="208" fill="{fg}" mask="url(#moon)"/>
  <path d="M676 300 l14 34 34 14 -34 14 -14 34 -14 -34 -34 -14 34 -14z" fill="{fg}" fill-opacity=".85"/>
  <path d="M300 744 L724 744" stroke="{fg}" stroke-opacity=".45" stroke-width="36" stroke-linecap="round"/>"""
    if app == "agent":
        # Background service: a steady core inside an open orbit.
        return f"""
  <path d="M512 290 A222 222 0 1 1 333 381" fill="none" stroke="{fg}" stroke-width="54" stroke-linecap="round"/>
  <circle cx="382" cy="318" r="34" fill="{fg}" fill-opacity=".6"/>
  <circle cx="512" cy="512" r="96" fill="{fg}"/>"""
    raise KeyError(app)


def svg(body: str, defs: str = "") -> str:
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">{defs}{body}\n</svg>\n'


def full_icon(app: str) -> str:
    light, deep = APPS[app]
    defs = f"""
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="{light}"/><stop offset="1" stop-color="{deep}"/>
    </linearGradient>
    <linearGradient id="sheen" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#fff" stop-opacity=".14"/><stop offset=".5" stop-color="#fff" stop-opacity="0"/>
    </linearGradient>
  </defs>"""
    bg = """
  <rect x="64" y="64" width="896" height="896" rx="208" fill="url(#bg)"/>
  <rect x="64" y="64" width="896" height="896" rx="208" fill="url(#sheen)"/>"""
    return svg(bg + glyph(app, W, deep), defs)


def foreground(app: str) -> str:
    return svg(glyph(app, W, APPS[app][1]))


def monochrome(app: str) -> str:
    # Cut-outs become transparent-looking by using the same colour as nothing behind;
    # Android tints this layer, so use black and let the system recolour it.
    return svg(glyph(app, "#000000", "#ffffff"))


def run(*cmd: str) -> None:
    subprocess.run(cmd, check=True)


def png(src: pathlib.Path, dst: pathlib.Path, size: int) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    run("rsvg-convert", "-w", str(size), "-h", str(size), "-o", str(dst), str(src))


ANDROID = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}


def main() -> None:
    SVG_DIR.mkdir(parents=True, exist_ok=True)
    tmp = ROOT / "ui/icons/.tmp"
    tmp.mkdir(exist_ok=True)
    for app in APPS:
        (SVG_DIR / f"{app}.svg").write_text(full_icon(app))
        (SVG_DIR / f"{app}-foreground.svg").write_text(foreground(app))
        (SVG_DIR / f"{app}-monochrome.svg").write_text(monochrome(app))

        out = ROOT / ("selfkit-agent/icons" if app == "agent" else f"apps/{app}/icons")
        full = SVG_DIR / f"{app}.svg"
        for name, size in [("32x32.png", 32), ("64x64.png", 64), ("128x128.png", 128), ("128x128@2x.png", 256), ("icon.png", 512)]:
            png(full, out / name, size)
        ico_parts = []
        for s in (16, 24, 32, 48, 64, 128, 256):
            p = tmp / f"{app}-{s}.png"
            png(full, p, s)
            ico_parts.append(str(p))
        run("magick", *ico_parts, str(out / "icon.ico"))

        if app == "agent":
            # Tray icons: monochrome glyph for light and dark bars.
            for s in (16, 22, 32, 44, 64):
                png(SVG_DIR / "agent-foreground.svg", out / f"tray-light-{s}.png", s)  # white glyph for dark bars
                png(SVG_DIR / "agent-monochrome.svg", out / f"tray-dark-{s}.png", s)   # black glyph for light bars
            continue

        for dens, s in ANDROID.items():
            d = out / "android" / f"mipmap-{dens}"
            png(full, d / "ic_launcher.png", s)
            png(full, d / "ic_launcher_round.png", s)
            # Adaptive foreground is 108dp with the 72dp visible area in the middle.
            png(SVG_DIR / f"{app}-foreground.svg", d / "ic_launcher_foreground.png", round(s * 108 / 48))
            png(SVG_DIR / f"{app}-monochrome.svg", d / "ic_launcher_monochrome.png", round(s * 108 / 48))
        (out / "android" / "ic_launcher_background.txt").write_text(APPS[app][1] + "\n")

    # Contact sheet: each icon at 256 px plus a 32 px version underneath.
    tiles = []
    for app in APPS:
        big = tmp / f"{app}-sheet.png"
        png(SVG_DIR / f"{app}.svg", big, 256)
        tiles.append(str(big))
    run("magick", "montage", *tiles, "-tile", f"{len(APPS)}x1", "-geometry", "256x256+24+24",
        "-background", "#f4f2ee", str(ROOT / "ui/icons/preview.png"))
    dark = []
    for app in APPS:
        small = tmp / f"{app}-small.png"
        png(SVG_DIR / f"{app}.svg", small, 48)
        dark.append(str(small))
    run("magick", "montage", *dark, "-tile", f"{len(APPS)}x1", "-geometry", "48x48+16+16",
        "-background", "#1f2226", str(ROOT / "ui/icons/preview-small-dark.png"))
    shutil.rmtree(tmp)
    print("icons written")


if __name__ == "__main__":
    main()
