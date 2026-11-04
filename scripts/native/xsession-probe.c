/*
 * Minimal X11 helper for the native capture session (scripts/native-capture.sh).
 * xsetroot/xdotool/xwininfo are not available on every verification host, so the three things
 * the capture script needs from X are done here directly:
 *
 *   bg dark|busy        - paint the root window (solid black / saturated mosaic) and stay
 *                         alive so the created pixmap is not destroyed on disconnect.
 *   list                - print "<id> <x> <y> <w> <h> <map_state> <name>" for every top-level
 *                         window (map_state 2 = IsViewable, i.e. actually on screen).
 *   sample <x> <y>      - print the root pixel at (x, y) as #rrggbb.
 *   type <text>         - inject keystrokes through XTEST (lowercase ASCII, digits, space).
 *   picheck <x y w h> <hex,hex,...> - count pixels inside the region that EXACTLY match any
 *                         of the given colours; used to prove an opaque window never lets the
 *                         busy background bleed into its client area.
 *   diff <x y w h> <delay-ms> - grab the region twice, <delay-ms> apart, and report how many
 *                         pixels changed (a settled, torn-free frame reports changed=0).
 *
 * Compile: cc -O2 xsession-probe.c -o xsession-probe $(pkg-config --cflags --libs x11 xtst)
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <unistd.h>
#include <X11/Xlib.h>
#include <X11/Xatom.h>
#include <X11/Xutil.h>
#include <X11/keysym.h>
#include <X11/extensions/XTest.h>

static unsigned long cell_color(int i) {
    /* Colours the application UI never paints: fully saturated primaries. The exact-match
     * bleed test relies on this list being disjoint from the app palette (checked by the
     * picheck result itself: a hit inside the window is a finding, not a false positive). */
    static const unsigned long cells[6] = {
        0xff00ffUL, 0x00ffffUL, 0xff0000UL, 0x00ff00UL, 0xffa500UL, 0x8000ffUL
    };
    return cells[i % 6];
}

static int parse_hex_color(const char *s, unsigned long *out) {
    char *end = NULL;
    unsigned long v;
    if (*s == '#') s++;
    if (*s == '\0') return 0;
    v = strtoul(s, &end, 16);
    if (end == s || *end != '\0') return 0;
    *out = v;
    return 1;
}

static int cmd_bg(int argc, char **argv) {
    Display *dpy = XOpenDisplay(NULL);
    if (!dpy) { fprintf(stderr, "probe: cannot open display\n"); return 2; }
    int scr = DefaultScreen(dpy);
    Window root = RootWindow(dpy, scr);
    unsigned w = (unsigned)DisplayWidth(dpy, scr);
    unsigned h = (unsigned)DisplayHeight(dpy, scr);
    int dark = argc > 1 && strcmp(argv[1], "dark") == 0;
    unsigned int cell = 40u;

    char *data = calloc((size_t)w * (size_t)h, sizeof(unsigned long));
    if (!data) { fprintf(stderr, "probe: out of memory\n"); return 2; }
    unsigned long *px = (unsigned long *)data;
    unsigned int y = 0, x = 0, seed = 20261102u;
    for (y = 0; y < h; y++) {
        for (x = 0; x < w; x++) {
            if (dark) {
                px[(size_t)y * w + x] = 0x000000UL;
            } else {
                /* deterministic pseudo-random mosaic of saturated cells over a mid-grey */
                unsigned int cx = x / cell, cy = y / cell;
                seed = seed * 1103515245u + 12345u;
                unsigned long col = (cx == cy) ? 0xffffffUL : 0x202020UL;
                if ((seed >> 24) < 96u) col = cell_color((int)(cx * 7u + cy * 13u));
                px[(size_t)y * w + x] = col;
            }
        }
    }
    XImage *img = XCreateImage(dpy, DefaultVisual(dpy, scr), (unsigned)DefaultDepth(dpy, scr),
                               ZPixmap, 0, data, w, h, 32, 0);
    Pixmap pm = XCreatePixmap(dpy, root, w, h, (unsigned)DefaultDepth(dpy, scr));
    GC gc = XCreateGC(dpy, root, 0, NULL);
    XPutImage(dpy, pm, gc, img, 0, 0, 0, 0, w, h);
    XSetWindowBackgroundPixmap(dpy, root, pm);
    Atom prop_root = XInternAtom(dpy, "_XROOTPMAP_ID", False);
    Atom prop_eset = XInternAtom(dpy, "ESETROOT_PMAP_ID", False);
    Pixmap props[1] = { pm };
    XChangeProperty(dpy, root, prop_root, XA_PIXMAP, 32, PropModeReplace,
                    (unsigned char *)props, 1);
    XChangeProperty(dpy, root, prop_eset, XA_PIXMAP, 32, PropModeReplace,
                    (unsigned char *)props, 1);
    XSetCloseDownMode(dpy, RetainPermanent);
    XClearWindow(dpy, root);
    XSync(dpy, False);
    /* Stay alive so the pixmap stays allocated even without RetainPermanent semantics. */
    for (;;) pause();
    return 0;
}

static void print_window(Display *dpy, Window win) {
    XWindowAttributes at;
    if (!XGetWindowAttributes(dpy, win, &at)) return;
    int rx = 0, ry = 0;
    Window child = 0;
    XTranslateCoordinates(dpy, win, RootWindow(dpy, DefaultScreen(dpy)), 0, 0, &rx, &ry, &child);
    char *name = NULL;
    unsigned char *data = NULL;
    Atom net_name = XInternAtom(dpy, "_NET_WM_NAME", False);
    Atom actual = 0; int fmt = 0; unsigned long n = 0, after = 0;
    if (XGetWindowProperty(dpy, win, net_name, 0, 256, False, AnyPropertyType, &actual, &fmt,
                           &n, &after, &data) == Success && data && n > 0) {
        name = (char *)data;
    } else if (!XFetchName(dpy, win, &name)) {
        name = NULL;
    }
    printf("0x%0lx %d %d %d %d %d %s\n", win, rx, ry, at.width, at.height, at.map_state,
           name ? name : "");
    if (data) XFree(data);
    else if (name) XFree(name);
}

static int cmd_list(int argc, char **argv) {
    (void)argc; (void)argv;
    Display *dpy = XOpenDisplay(NULL);
    if (!dpy) { fprintf(stderr, "probe: cannot open display\n"); return 2; }
    Window root = RootWindow(dpy, DefaultScreen(dpy));
    Window parent = 0, *kids = NULL; unsigned n = 0;
    XQueryTree(dpy, root, &root, &parent, &kids, &n);
    for (unsigned i = 0; i < n; i++) print_window(dpy, kids[i]);
    if (kids) XFree(kids);
    XCloseDisplay(dpy);
    return 0;
}

static int cmd_sample(int argc, char **argv) {
    if (argc < 3) { fprintf(stderr, "usage: sample x y\n"); return 2; }
    int x = atoi(argv[1]), y = atoi(argv[2]);
    Display *dpy = XOpenDisplay(NULL);
    if (!dpy) { fprintf(stderr, "probe: cannot open display\n"); return 2; }
    if (x < 0 || y < 0 || x >= DisplayWidth(dpy, DefaultScreen(dpy)) ||
        y >= DisplayHeight(dpy, DefaultScreen(dpy))) {
        fprintf(stderr, "probe: sample outside the screen\n");
        return 2;
    }
    Window root = RootWindow(dpy, DefaultScreen(dpy));
    XImage *img = XGetImage(dpy, root, x, y, 1, 1, AllPlanes, ZPixmap);
    if (!img) { fprintf(stderr, "probe: no pixel\n"); return 1; }
    unsigned long v = XGetPixel(img, 0, 0);
    printf("#%02lx%02lx%02lx\n", (v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff);
    XDestroyImage(img);
    XCloseDisplay(dpy);
    return 0;
}

static int cmd_focus(int argc, char **argv) {
    if (argc < 2) { fprintf(stderr, "usage: focus <window-id>\n"); return 2; }
    Window win = (Window)strtoul(argv[1], NULL, 0);
    Display *dpy = XOpenDisplay(NULL);
    if (!dpy) { fprintf(stderr, "probe: cannot open display\n"); return 2; }
    XSetInputFocus(dpy, win, RevertToParent, CurrentTime);
    XAllowEvents(dpy, AsyncKeyboard, CurrentTime);
    XSync(dpy, False);
    XCloseDisplay(dpy);
    return 0;
}

static int cmd_type(int argc, char **argv) {
    if (argc < 2) { fprintf(stderr, "usage: type <window-id|-> text\n"); return 2; }
    Window win = 0;
    const char *text = argv[2];
    if (strcmp(argv[1], "-") != 0) win = (Window)strtoul(argv[1], NULL, 0);
    Display *dpy = XOpenDisplay(NULL);
    if (!dpy) { fprintf(stderr, "probe: cannot open display\n"); return 2; }
    int ev = 0, err = 0, major = 0, minor = 0;
    if (!XTestQueryExtension(dpy, &ev, &err, &major, &minor)) {
        fprintf(stderr, "probe: XTEST unavailable\n"); return 2;
    }
    if (win) {
        XSetInputFocus(dpy, win, RevertToParent, CurrentTime);
        XAllowEvents(dpy, AsyncKeyboard, CurrentTime);
        XSync(dpy, False);
        usleep(250000);
    }
    for (const char *s = text; *s; s++) {
        char ch = *s;
        if (!(ch == ' ' || (ch >= 'a' && ch <= 'z') || (ch >= '0' && ch <= '9'))) {
            fprintf(stderr, "probe: refusing unsupported char '%c'\n", ch);
            XCloseDisplay(dpy);
            return 2;
        }
        KeySym ks = (ch == ' ') ? XK_space
                   : ((ch >= '0' && ch <= '9') ? XK_0 + (ch - '0') : XK_a + (ch - 'a'));
        KeyCode kc = XKeysymToKeycode(dpy, ks);
        if (!kc) { fprintf(stderr, "probe: no keycode for '%c'\n", ch); continue; }
        XTestFakeKeyEvent(dpy, kc, True, 0);
        XTestFakeKeyEvent(dpy, kc, False, 0);
    }
    XSync(dpy, False);
    XCloseDisplay(dpy);
    return 0;
}

static int cmd_picheck(int argc, char **argv) {
    if (argc < 6) { fprintf(stderr, "usage: picheck x y w h hex,hex,...\n"); return 2; }
    int x = atoi(argv[1]), y = atoi(argv[2]);
    unsigned w = (unsigned)atoi(argv[3]), h = (unsigned)atoi(argv[4]);
    unsigned long wanted[16];
    int nw = 0;
    char *list = strdup(argv[5]);
    for (char *tok = strtok(list, ","); tok && nw < 16; tok = strtok(NULL, ",")) {
        if (!parse_hex_color(tok, &wanted[nw++])) {
            fprintf(stderr, "probe: bad colour %s\n", tok); return 2;
        }
    }
    Display *dpy = XOpenDisplay(NULL);
    if (!dpy) { fprintf(stderr, "probe: cannot open display\n"); return 2; }
    Window root = RootWindow(dpy, DefaultScreen(dpy));
    XImage *img = XGetImage(dpy, root, x, y, w, h, AllPlanes, ZPixmap);
    if (!img) { fprintf(stderr, "probe: no region\n"); return 1; }
    long hits[16];
    for (int i = 0; i < nw; i++) hits[i] = 0;
    for (unsigned py = 0; py < h; py++) {
        for (unsigned px = 0; px < w; px++) {
            unsigned long v = XGetPixel(img, (int)px, (int)py);
            for (int i = 0; i < nw; i++) if (v == wanted[i]) { hits[i]++; break; }
        }
    }
    for (int i = 0; i < nw; i++) printf("%06lx:%ld\n", wanted[i], hits[i]);
    XDestroyImage(img);
    XCloseDisplay(dpy);
    return 0;
}

static int cmd_diff(int argc, char **argv) {
    if (argc < 6) { fprintf(stderr, "usage: diff x y w h delay-ms\n"); return 2; }
    int x = atoi(argv[1]), y = atoi(argv[2]);
    unsigned w = (unsigned)atoi(argv[3]), h = (unsigned)atoi(argv[4]);
    long ms = atol(argv[5]);
    Display *dpy = XOpenDisplay(NULL);
    if (!dpy) { fprintf(stderr, "probe: cannot open display\n"); return 2; }
    Window root = RootWindow(dpy, DefaultScreen(dpy));
    XImage *a = XGetImage(dpy, root, x, y, w, h, AllPlanes, ZPixmap);
    if (!a) { fprintf(stderr, "probe: no region\n"); return 1; }
    usleep((useconds_t)ms * 1000);
    XImage *b = XGetImage(dpy, root, x, y, w, h, AllPlanes, ZPixmap);
    if (!b) { fprintf(stderr, "probe: no region (2)\n"); XDestroyImage(a); return 1; }
    long changed = 0; int minx = (int)w, miny = (int)h, maxx = -1, maxy = -1;
    for (unsigned py = 0; py < h; py++) {
        for (unsigned px = 0; px < w; px++) {
            if (XGetPixel(a, (int)px, (int)py) != XGetPixel(b, (int)px, (int)py)) {
                changed++;
                if ((int)px < minx) minx = (int)px;
                if ((int)px > maxx) maxx = (int)px;
                if ((int)py < miny) miny = (int)py;
                if ((int)py > maxy) maxy = (int)py;
            }
        }
    }
    if (changed) {
        printf("changed=%ld bbox=%d,%d..%d,%d\n", changed, x + minx, y + miny, x + maxx, y + maxy);
    } else {
        printf("changed=0\n");
    }
    XDestroyImage(a);
    XDestroyImage(b);
    XCloseDisplay(dpy);
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: xsession-probe bg dark|busy | list | sample x y | type win text |"
                        " picheck x y w h colours | diff x y w h delay-ms\n");
        return 2;
    }
    if (strcmp(argv[1], "bg") == 0) return cmd_bg(argc - 1, argv + 1);
    if (strcmp(argv[1], "list") == 0) return cmd_list(argc - 1, argv + 1);
    if (strcmp(argv[1], "sample") == 0) return cmd_sample(argc - 1, argv + 1);
    if (strcmp(argv[1], "focus") == 0) return cmd_focus(argc - 1, argv + 1);
    if (strcmp(argv[1], "type") == 0) return cmd_type(argc - 1, argv + 1);
    if (strcmp(argv[1], "picheck") == 0) return cmd_picheck(argc - 1, argv + 1);
    if (strcmp(argv[1], "diff") == 0) return cmd_diff(argc - 1, argv + 1);
    fprintf(stderr, "probe: unknown subcommand %s\n", argv[1]);
    return 2;
}
