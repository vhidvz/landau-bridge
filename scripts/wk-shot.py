#!/usr/bin/env python3
"""Snapshot a URL with WebKitGTK (webkit2gtk-4.1) — the engine Tauri uses on Linux.

Chrome/headless Chromium stills do not catch WebKit-only text bugs (font shaping,
line-box metrics), so design review of the desktop app needs a WebKit capture.

Usage:
    scripts/wk-shot.py URL OUT.png [WIDTH HEIGHT] [--settle MS] [--visible]

    URL         http(s):// or file:// — e.g. http://localhost:3210/settings?state=done&fx=full
    OUT.png     where to write the PNG
    WIDTH       viewport width  (default 1440)
    HEIGHT      viewport height (default 900)
    --settle    extra milliseconds to wait after the DOM goes quiet (default 1200)
                so fonts and the last transitions have landed
    --nowait    skip the quiet-DOM wait and settle straight after load-finished.
                By default the script polls document.body.innerHTML.length every
                200 ms and waits for three equal readings (capped at 15 s): the
                app hydrates and fetches after load-finished, and in WebKit that
                lands at an unpredictable moment, so a fixed delay alone gives
                half-populated stills from one run to the next.
    --visible   snapshot only the viewport instead of the full document
    --motion    keep animations on. Off by default: the script turns GTK
                animations off, which WebKit reports as `prefers-reduced-motion:
                reduce`, so the rAF-driven page-enter reveal lands at once — the
                same thing scripts/shots.sh asks Chrome for with
                --force-prefers-reduced-motion. Without it an off-screen window
                gets its frames throttled and <main> snapshots half-revealed.

Needs a display: DISPLAY=:0 (Xwayland is fine) and GDK_BACKEND=x11.
The window is mapped far off-screen (-4000,-4000) so nothing appears on a workspace
while the compositor still drives requestAnimationFrame — the page-enter reveal is
rAF-driven, and in a truly unmapped window it never runs and <main> snapshots blank.
"""

import argparse
import sys
import os

os.environ.setdefault("GDK_BACKEND", "x11")

import gi  # noqa: E402

gi.require_version("Gdk", "3.0")
gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import Gdk, Gtk, WebKit2, GLib  # noqa: E402


def parse_args(argv):
    parser = argparse.ArgumentParser(
        prog="wk-shot.py",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("url", help="http(s):// or file:// URL to load")
    parser.add_argument("out", metavar="OUT.png", help="where to write the PNG")
    parser.add_argument("width", nargs="?", type=int, default=1440,
                        help="viewport width (default 1440)")
    parser.add_argument("height", nargs="?", type=int, default=900,
                        help="viewport height (default 900)")
    parser.add_argument("--settle", type=int, default=1200, metavar="MS",
                        help="ms to wait before the snapshot (default 1200)")
    parser.add_argument("--nowait", action="store_true",
                        help="skip the quiet-DOM wait")
    parser.add_argument("--visible", action="store_true",
                        help="snapshot the viewport, not the full document")
    parser.add_argument("--motion", action="store_true",
                        help="keep animations on")
    return parser.parse_args(argv[1:])


def main(argv):
    opts = parse_args(argv)
    url, out = opts.url, opts.out
    width, height = opts.width, opts.height
    settle_ms = opts.settle

    region = (
        WebKit2.SnapshotRegion.VISIBLE
        if opts.visible
        else WebKit2.SnapshotRegion.FULL_DOCUMENT
    )

    print(
        f"wk-shot: {width}x{height}, settle {settle_ms}ms, "
        f"{'viewport' if opts.visible else 'full-document'}, "
        f"{'motion' if opts.motion else 'reduced-motion'}, "
        f"{'no quiet-DOM wait' if opts.nowait else 'quiet-DOM wait'}",
        file=sys.stderr,
    )

    if not Gtk.init_check(None)[0]:
        print("wk-shot: cannot open a display (need DISPLAY=:0)", file=sys.stderr)
        return 3

    if not opts.motion:
        gtk_settings = Gtk.Settings.get_default()
        if gtk_settings is not None:
            gtk_settings.set_property("gtk-enable-animations", False)

    # A real, mapped window (so the compositor keeps driving rAF and CSS
    # transitions) parked far off-screen, undecorated and skipping the taskbar.
    win = Gtk.Window(type=Gtk.WindowType.TOPLEVEL)
    win.set_default_size(width, height)
    win.set_decorated(False)
    win.set_skip_taskbar_hint(True)
    win.set_skip_pager_hint(True)
    win.set_type_hint(Gdk.WindowTypeHint.UTILITY)
    win.move(-4000, -4000)

    view = WebKit2.WebView()
    view.set_size_request(width, height)
    settings = view.get_settings()
    settings.set_enable_developer_extras(True)
    settings.set_enable_write_console_messages_to_stdout(False)
    win.add(view)
    win.show_all()
    win.move(-4000, -4000)

    state = {"rc": 1}

    def on_snapshot_done(view_, result):
        try:
            surface = view_.get_snapshot_finish(result)
            surface.write_to_png(out)
            w, h = surface.get_width(), surface.get_height()
            print(f"wk-shot: wrote {out} ({w}x{h})")
            state["rc"] = 0
        except GLib.Error as err:
            print(f"wk-shot: snapshot failed: {err.message}", file=sys.stderr)
        except Exception as err:  # noqa: BLE001
            print(f"wk-shot: snapshot failed: {err}", file=sys.stderr)
        Gtk.main_quit()

    def take_snapshot():
        view.get_snapshot(region, WebKit2.SnapshotOptions.NONE, None, on_snapshot_done)
        return False

    # --- wait for the DOM to stop changing -----------------------------------
    quiet = {"last": None, "same": 0, "polls": 0}
    POLL_MS, NEEDED_SAME, MAX_POLLS = 200, 3, 75  # 75 * 200 ms = 15 s ceiling

    def on_measured(view_, result):
        try:
            value = view_.evaluate_javascript_finish(result)
            size = value.to_int32() if value is not None else -1
        except Exception:  # noqa: BLE001 — a navigating page can refuse the eval
            size = -1
        if size >= 0 and size == quiet["last"]:
            quiet["same"] += 1
        else:
            quiet["same"] = 0
        quiet["last"] = size
        if quiet["same"] >= NEEDED_SAME or quiet["polls"] >= MAX_POLLS:
            if quiet["polls"] >= MAX_POLLS:
                print("wk-shot: DOM still changing after 15s, snapshotting anyway",
                      file=sys.stderr)
            GLib.timeout_add(settle_ms, take_snapshot)
        else:
            GLib.timeout_add(POLL_MS, poll_quiet)

    def poll_quiet():
        quiet["polls"] += 1
        view.evaluate_javascript(
            "document.body ? document.body.innerHTML.length : -1",
            -1, None, None, None, on_measured,
        )
        return False

    def on_load_changed(view_, event):
        if event == WebKit2.LoadEvent.FINISHED:
            if opts.nowait:
                GLib.timeout_add(settle_ms, take_snapshot)
            else:
                GLib.timeout_add(POLL_MS, poll_quiet)

    def on_load_failed(view_, event, failing_uri, error):
        print(f"wk-shot: load failed for {failing_uri}: {error.message}", file=sys.stderr)
        Gtk.main_quit()
        return True

    view.connect("load-changed", on_load_changed)
    view.connect("load-failed", on_load_failed)

    # Hard ceiling so a hung page can never wedge a review run.
    def on_hard_timeout():
        print("wk-shot: hard 60s ceiling hit, quitting", file=sys.stderr)
        Gtk.main_quit()
        return False

    GLib.timeout_add_seconds(60, on_hard_timeout)

    view.load_uri(url)
    Gtk.main()
    return state["rc"]


if __name__ == "__main__":
    sys.exit(main(sys.argv))
