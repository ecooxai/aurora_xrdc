/* SPDX-License-Identifier: GPL-3.0-or-later
 * Memory framebuffer DDX for tinycorelinux/tinyx.
 * No framebuffer device, VT, physical input device, or root privileges.
 * Keyboard/pointer input is supplied exclusively by authenticated X11/XTEST.
 */
#ifdef HAVE_CONFIG_H
#include <kdrive-config.h>
#endif
#include "kdrive.h"
#include "kkeymap.h"
#include <X11/keysym.h>
#include <stdlib.h>
#include <string.h>

static const KdOsFuncs memoryOs = {0};
void OsVendorInit(void) { KdOsInit(&memoryOs); }
static Bool cardInit(KdCardInfo *card) { (void)card; return TRUE; }
static Bool screenInit(KdScreenInfo *s) {
    if (!s->width) s->width = 1280;
    if (!s->height) s->height = 720;
    if (s->width < 64 || s->width > 8192 || s->height < 64 || s->height > 8192) return FALSE;
    s->rate = 60; s->randr = RR_Rotate_0;
    s->dumb = TRUE; s->softCursor = TRUE;
    s->fb.depth = 24; s->fb.bitsPerPixel = 32;
    s->fb.pixelStride = s->width; s->fb.byteStride = s->width * 4;
    s->fb.visuals = (1 << TrueColor);
    s->fb.redMask = 0xff0000; s->fb.greenMask = 0x00ff00; s->fb.blueMask = 0x0000ff;
    s->fb.shadow = FALSE;
    s->memory_size = (unsigned long)s->fb.byteStride * s->height;
    s->memory_base = s->fb.frameBuffer = calloc(1, s->memory_size);
    return s->fb.frameBuffer != NULL;
}
static void screenFini(KdScreenInfo *s) { free(s->fb.frameBuffer); s->fb.frameBuffer = NULL; }
static Bool screenEnable(ScreenPtr s) { (void)s; return TRUE; }
static Bool dpms(ScreenPtr s, int mode) { (void)s; (void)mode; return TRUE; }
static void colors(ScreenPtr s, int n, xColorItem *c) { (void)s; (void)n; (void)c; }
static const KdCardFuncs memoryCard = {
    .cardinit = cardInit, .scrinit = screenInit, .enable = screenEnable,
    .dpms = dpms, .scrfini = screenFini, .getColors = colors, .putColors = colors
};
void InitCard(char *name) { (void)name; KdCardInfoAdd(&memoryCard, NULL); }
void InitOutput(ScreenInfo *s, int argc, char **argv) { KdInitOutput(s, argc, argv); }

static void key(unsigned int scan, KeySym plain, KeySym shift) {
    kdKeymap[scan * KD_MAX_WIDTH] = plain;
    kdKeymap[scan * KD_MAX_WIDTH + 1] = shift ? shift : plain;
}
static void loadKeys(void) {
    unsigned int i;
    const char *lower[] = {"1234567890-=", "qwertyuiop[]", "asdfghjkl;'", "zxcvbnm,./"};
    const char *upper[] = {"!@#$%^&*()_+", "QWERTYUIOP{}", "ASDFGHJKL:\"", "ZXCVBNM<>?"};
    const unsigned int starts[] = {2,16,30,44};
    memset(kdKeymap, 0, sizeof(KeySym)*KD_MAX_LENGTH*KD_MAX_WIDTH);
    kdMinScanCode = 0; kdMaxScanCode = 127;
    for (i=0; i<4; i++) for (unsigned int j=0; lower[i][j]; j++) key(starts[i]+j,lower[i][j],upper[i][j]);
    key(1,XK_Escape,0); key(14,XK_BackSpace,0); key(15,XK_Tab,XK_ISO_Left_Tab);
    key(28,XK_Return,0); key(29,XK_Control_L,0); key(41,'`','~');
    key(42,XK_Shift_L,0); key(43,'\\','|'); key(54,XK_Shift_R,0);
    key(55,XK_KP_Multiply,0); key(56,XK_Alt_L,0); key(57,XK_space,0); key(58,XK_Caps_Lock,0);
    for (i=0;i<10;i++) key(59+i,XK_F1+i,0);
    key(69,XK_Num_Lock,0); key(70,XK_Scroll_Lock,0);
    key(71,XK_KP_Home,XK_KP_7); key(72,XK_KP_Up,XK_KP_8); key(73,XK_KP_Prior,XK_KP_9);
    key(74,XK_KP_Subtract,0); key(75,XK_KP_Left,XK_KP_4); key(76,XK_KP_Begin,XK_KP_5);
    key(77,XK_KP_Right,XK_KP_6); key(78,XK_KP_Add,0); key(79,XK_KP_End,XK_KP_1);
    key(80,XK_KP_Down,XK_KP_2); key(81,XK_KP_Next,XK_KP_3); key(82,XK_KP_Insert,XK_KP_0);
    key(83,XK_KP_Delete,XK_KP_Decimal); key(87,XK_F11,0); key(88,XK_F12,0);
    key(96,XK_KP_Enter,0); key(97,XK_Control_R,0); key(98,XK_KP_Divide,0);
    key(99,XK_Print,0); key(100,XK_Alt_R,0); key(102,XK_Home,0); key(103,XK_Up,0);
    key(104,XK_Page_Up,0); key(105,XK_Left,0); key(106,XK_Right,0); key(107,XK_End,0);
    key(108,XK_Down,0); key(109,XK_Page_Down,0); key(110,XK_Insert,0); key(111,XK_Delete,0);
    key(119,XK_Pause,0); key(125,XK_Super_L,0); key(126,XK_Super_R,0); key(127,XK_Menu,0);
}
static int keyboardInit(void) { return -1; }
static Bool mouseInit(void) { return TRUE; }
static void noop(void) {}
static void leds(int n) { (void)n; }
static void bell(int a, int b, int c) { (void)a; (void)b; (void)c; }
static const KdMouseFuncs mouseFuncs = {mouseInit, noop};
static const KdKeyboardFuncs keyboardFuncs = {loadKeys, keyboardInit, leds, bell, noop, 0};
void InitInput(int argc, char **argv) {
    (void)argc; (void)argv;
    KdMouseInfo *m = KdMouseInfoAdd();
    if (!m) FatalError("unable to allocate virtual pointer\n");
    m->nbutton=7; for (int i=0;i<7;i++) m->map[i]=i+1;
    m->emulateMiddleButton=FALSE;
    KdInitInput(&mouseFuncs, &keyboardFuncs);
}
void ddxUseMsg(void) { KdUseMsg(); ErrorF("\nXtiny: TinyX in-memory framebuffer; no hardware access\n"); }
int ddxProcessArgument(int argc, char **argv, int i) {
    if (!strcmp(argv[i], "-version")) { kdVersion("Xtiny (TinyX memory framebuffer)"); exit(0); }
    return KdProcessArgument(argc, argv, i);
}
