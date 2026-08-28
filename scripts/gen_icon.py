#!/usr/bin/env python3
"""MotionDesk 앱 아이콘 생성: 그라데이션 라운드 사각형 + 어두운 원 + 위/아래 셰브론.
사용: python3 scripts/gen_icon.py  →  desk-tray/assets/icon.icns
필요: Pillow, iconutil(macOS 기본)
"""
import pathlib
import subprocess
import tempfile

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "desk-tray" / "assets"
S = 4096  # 4배 해상도로 그린 뒤 축소 (안티앨리어싱)


def hx(c):
    return tuple(int(c[i : i + 2], 16) for i in (1, 3, 5))


def main():
    # 라운드 사각형 (macOS 아이콘 그리드: 1024 중 여백 100, 코너 반경 ~185) — 그레이 단색
    margin, radius = int(S * 100 / 1024), int(S * 185 / 1024)
    icon = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(icon)
    d.rounded_rectangle(
        [margin, margin, S - margin, S - margin], radius=radius, fill=hx("#8e8e93")
    )
    # 어두운 원
    cr = int(S * 330 / 1024)
    c = S // 2
    d.ellipse([c - cr, c - cr, c + cr, c + cr], fill=(43, 43, 46, 255))

    # 위/아래 셰브론 (둥근 끝 두꺼운 선) — 참고 디자인처럼 넓고 낮은 비율
    w = int(S * 68 / 1024)  # 선 두께
    half = int(S * 118 / 1024)  # 화살 절반 너비
    h = int(S * 64 / 1024)  # 화살 높이
    gap = int(S * 48 / 1024)

    def chevron(apex_y, up):
        sign = 1 if up else -1
        pts = [(c - half, apex_y + sign * h), (c, apex_y), (c + half, apex_y + sign * h)]
        d.line(pts, fill=(255, 255, 255, 255), width=w, joint="curve")
        for x, y in (pts[0], pts[2]):
            d.ellipse([x - w // 2, y - w // 2, x + w // 2, y + w // 2], fill=(255, 255, 255, 255))

    chevron(c - gap - h, up=True)
    chevron(c + gap + h, up=False)

    icon = icon.resize((1024, 1024), Image.LANCZOS)
    OUT.mkdir(parents=True, exist_ok=True)
    icon.save(OUT / "icon_preview.png")

    # iconset → icns
    with tempfile.TemporaryDirectory() as td:
        iconset = pathlib.Path(td) / "icon.iconset"
        iconset.mkdir()
        for size in (16, 32, 128, 256, 512):
            icon.resize((size, size), Image.LANCZOS).save(iconset / f"icon_{size}x{size}.png")
            icon.resize((size * 2, size * 2), Image.LANCZOS).save(
                iconset / f"icon_{size}x{size}@2x.png"
            )
        subprocess.run(
            ["iconutil", "-c", "icns", str(iconset), "-o", str(OUT / "icon.icns")], check=True
        )
    print(f"완료: {OUT / 'icon.icns'}")


if __name__ == "__main__":
    main()
