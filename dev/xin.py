#!/usr/bin/env python3
"""Tiny xdotool stand-in (XTEST via ctypes) for driving apps under Xvfb.
usage: DISPLAY=:82 dev/xin.py move X Y | click X Y [button] | key KEYSYM [KEYSYM...] | type TEXT | sleep S
Several commands can be chained with ' -- '."""
import ctypes, sys, time

x11 = ctypes.cdll.LoadLibrary("libX11.so.6")
xt = ctypes.cdll.LoadLibrary("libXtst.so.6")
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XStringToKeysym.restype = ctypes.c_ulong
x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
d = ctypes.c_void_p(x11.XOpenDisplay(None))
if not d.value:
    sys.exit("cannot open display")

def flush():
    x11.XFlush(d)

def move(x, y):
    xt.XTestFakeMotionEvent(d, -1, int(x), int(y), 0); flush()

def click(x, y, b=1):
    move(x, y); time.sleep(0.05)
    xt.XTestFakeButtonEvent(d, int(b), 1, 0); flush(); time.sleep(0.05)
    xt.XTestFakeButtonEvent(d, int(b), 0, 0); flush()

def kc(sym):
    return x11.XKeysymToKeycode(d, x11.XStringToKeysym(sym.encode()))

def key(*syms):
    # "ctrl+Return" style combos
    for s in syms:
        parts = s.split("+")
        mods = [{"ctrl": "Control_L", "shift": "Shift_L", "alt": "Alt_L", "super": "Super_L"}.get(p, p) for p in parts[:-1]]
        for m in mods: xt.XTestFakeKeyEvent(d, kc(m), 1, 0)
        xt.XTestFakeKeyEvent(d, kc(parts[-1]), 1, 0); xt.XTestFakeKeyEvent(d, kc(parts[-1]), 0, 0)
        for m in reversed(mods): xt.XTestFakeKeyEvent(d, kc(m), 0, 0)
        flush(); time.sleep(0.03)

NAMES = {" ": "space", ".": "period", ",": "comma", "-": "minus", "!": "exclam", "?": "question", "'": "apostrophe", ":": "colon", "\n": "Return", "/": "slash", "_": "underscore", "@": "at", "(": "parenleft", ")": "parenright", "~": "asciitilde"}

def type_(text):
    for ch in text:
        sym = NAMES.get(ch, ch)
        shift = ch.isupper() or ch in "!?:_@()~"
        if shift: xt.XTestFakeKeyEvent(d, kc("Shift_L"), 1, 0)
        xt.XTestFakeKeyEvent(d, kc(sym), 1, 0); xt.XTestFakeKeyEvent(d, kc(sym), 0, 0)
        if shift: xt.XTestFakeKeyEvent(d, kc("Shift_L"), 0, 0)
        flush(); time.sleep(0.02)

args = sys.argv[1:]
cmds, cur = [], []
for a in args:
    if a == "--": cmds.append(cur); cur = []
    else: cur.append(a)
cmds.append(cur)
for c in cmds:
    if not c: continue
    op, rest = c[0], c[1:]
    if op == "move": move(*rest)
    elif op == "click": click(*rest)
    elif op == "key": key(*rest)
    elif op == "type": type_(" ".join(rest))
    elif op == "sleep": time.sleep(float(rest[0]))
