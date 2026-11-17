//! The island's window: a transparent panel over the menu bar, with the
//! core's silhouette hanging from the top edge. The pointer opens it on the
//! core's springs; open, it shows the time.

use std::cell::{Cell, OnceCell, RefCell};
use std::time::Instant;

use island_core::media::OPEN_WIDTH;
use island_core::pages::OPEN_HEIGHT;
use island_core::settings::Language;
use island_core::shape::{self, Placement, Rect, Segment};
use island_core::spring::{self, Spring};
use island_core::texts::texts;
use objc2::rc::Retained;
use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSBezierPath, NSColor,
    NSEvent, NSFont, NSMenu, NSMenuItem, NSPanel, NSScreen, NSTextAlignment, NSTextField,
    NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{
    NSDate, NSDateFormatter, NSDateFormatterStyle, NSLocale, NSPoint, NSRect, NSSize, NSString,
    NSTimer,
};

use crate::geometry::arc_corner;

// The Windows app's sizes, in points.
const CLOSED: (f32, f32) = (180.0, 32.0);
const OPEN: (f32, f32) = (OPEN_WIDTH, OPEN_HEIGHT);
const RADIUS: f32 = 18.0;
const EAR: f32 = 10.0;
const PLACEMENT: Placement = Placement::Top;
/// Over the menu bar (`NSStatusWindowLevel`).
const LEVEL: isize = 25;
const FRAME_S: f64 = 1.0 / 60.0;
const CLOCK_HEIGHT: f64 = 48.0;

struct Ivars {
    width: Cell<Spring>,
    height: Cell<Spring>,
    open: Cell<bool>,
    last: Cell<Instant>,
    timer: RefCell<Option<Retained<NSTimer>>>,
    clock: OnceCell<Retained<NSTextField>>,
    time: Retained<NSDateFormatter>,
    quit: &'static str,
}

define_class!(
    // SAFETY: NSView allows subclassing, and the view doesn't implement Drop.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "IslandView"]
    #[ivars = Ivars]
    struct IslandView;

    impl IslandView {
        /// y grows down, like the core's outline.
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let (w, h) = (self.ivars().width.get().value, self.ivars().height.get().value);
            let body = self.body(w, h, self.ivars().width.get().is_settled());
            let outline = shape::hanging(w, h, RADIUS, EAR);
            let at = |p: shape::Point| NSPoint::new((body.x + p.x) as f64, p.y as f64);
            let path = NSBezierPath::bezierPath();
            path.moveToPoint(at(outline.start));
            let mut from = outline.start;
            for segment in outline.segments {
                match segment {
                    Segment::Line(to) => {
                        path.lineToPoint(at(to));
                        from = to;
                    }
                    Segment::Cubic { c1, c2, to } => {
                        path.curveToPoint_controlPoint1_controlPoint2(at(to), at(c1), at(c2));
                        from = to;
                    }
                    Segment::Arc { to, radius, clockwise } => {
                        let corner = arc_corner(from, to, clockwise);
                        path.appendBezierPathWithArcFromPoint_toPoint_radius(
                            at(corner),
                            at(to),
                            radius as f64,
                        );
                        from = to;
                    }
                }
            }
            path.closePath();
            NSColor::blackColor().setFill();
            path.fill();
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            let p = self.convertPoint_fromView(event.locationInWindow(), None);
            // Over the body it is heading to: the pill opens it, and the
            // whole open island keeps it open.
            let (w, h) = if self.ivars().open.get() { OPEN } else { CLOSED };
            let body = self.body(w, h, true);
            let over = p.x >= body.x as f64
                && p.x < (body.x + body.width) as f64
                && p.y >= 0.0
                && p.y < h as f64;
            self.set_open(over);
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.set_open(false);
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            let mtm = self.mtm();
            let menu = NSMenu::new(mtm);
            // No target: `terminate:` reaches the app through the responders.
            let quit = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(self.ivars().quit),
                    Some(sel!(terminate:)),
                    &NSString::new(),
                )
            };
            menu.addItem(&quit);
            NSMenu::popUpContextMenu_withEvent_forView(&menu, event, self);
        }

        #[unsafe(method(tick:))]
        fn tick(&self, _timer: &NSTimer) {
            let iv = self.ivars();
            let now = Instant::now();
            let elapsed = now.duration_since(iv.last.replace(now)).as_secs_f32() * 1000.0;
            let open = iv.open.get();
            let (pw, ph) = if open {
                (spring::OPEN_WIDTH, spring::OPEN_HEIGHT)
            } else {
                (spring::CLOSE_WIDTH, spring::CLOSE_HEIGHT)
            };
            let (mut w, mut h) = (iv.width.get(), iv.height.get());
            let moving = w.advance(&pw, elapsed) | h.advance(&ph, elapsed);
            iv.width.set(w);
            iv.height.set(h);
            self.lay_out();
            // Nothing runs at rest: closed, no timer; open, one a second
            // for the clock.
            match (moving, open) {
                (true, _) => {}
                (false, true) => self.schedule(Some(1.0)),
                (false, false) => self.schedule(None),
            }
        }
    }
);

impl IslandView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let time = NSDateFormatter::new();
        time.setDateStyle(NSDateFormatterStyle::NoStyle);
        time.setTimeStyle(NSDateFormatterStyle::ShortStyle);
        let locale = NSLocale::currentLocale().localeIdentifier().to_string();
        let this = Self::alloc(mtm).set_ivars(Ivars {
            width: Cell::new(Spring::at(CLOSED.0)),
            height: Cell::new(Spring::at(CLOSED.1)),
            open: Cell::new(false),
            last: Cell::new(Instant::now()),
            timer: RefCell::new(None),
            clock: OnceCell::new(),
            time,
            quit: texts(Language::System, &locale).quit,
        });
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };

        let clock = NSTextField::labelWithString(&NSString::new(), mtm);
        clock.setTextColor(Some(&NSColor::whiteColor()));
        clock.setFont(Some(&NSFont::boldSystemFontOfSize(40.0)));
        clock.setAlignment(NSTextAlignment::Center);
        this.addSubview(&clock);
        let _ = this.ivars().clock.set(clock);

        let tracking = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                frame,
                NSTrackingAreaOptions::MouseEnteredAndExited
                    | NSTrackingAreaOptions::MouseMoved
                    | NSTrackingAreaOptions::ActiveAlways
                    | NSTrackingAreaOptions::InVisibleRect,
                Some(&this),
                None,
            )
        };
        this.addTrackingArea(&tracking);
        this.lay_out();
        this
    }

    /// The body (ears included) inside the view.
    fn body(&self, width: f32, height: f32, snap: bool) -> Rect {
        let window = self.bounds().size.width as f32;
        shape::body_rect(PLACEMENT, window, width, height, EAR, snap)
    }

    fn set_open(&self, open: bool) {
        let iv = self.ivars();
        if iv.open.replace(open) == open {
            return;
        }
        let (tw, th) = if open { OPEN } else { CLOSED };
        let (mut w, mut h) = (iv.width.get(), iv.height.get());
        w.target = tw;
        h.target = th;
        iv.width.set(w);
        iv.height.set(h);
        iv.last.set(Instant::now());
        self.schedule(Some(FRAME_S));
    }

    /// Replaces the timer: one every `interval` seconds, or none.
    fn schedule(&self, interval: Option<f64>) {
        let mut timer = self.ivars().timer.borrow_mut();
        if let Some(old) = timer.take() {
            if interval == Some(old.timeInterval()) {
                *timer = Some(old);
                return;
            }
            old.invalidate();
        }
        *timer = interval.map(|s| unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                s,
                self,
                sel!(tick:),
                None,
                true,
            )
        });
    }

    /// The clock follows the body and fades in as it opens.
    fn lay_out(&self) {
        let iv = self.ivars();
        let (w, h) = (iv.width.get().value, iv.height.get().value);
        if let Some(clock) = iv.clock.get() {
            let openness = ((h - CLOSED.1) / (OPEN.1 - CLOSED.1)).clamp(0.0, 1.0);
            clock.setAlphaValue(openness as f64);
            if openness > 0.0 {
                clock.setStringValue(&iv.time.stringFromDate(&NSDate::now()));
            }
            let body = self.body(w, h, false);
            clock.setFrame(NSRect::new(
                NSPoint::new(body.x as f64, (h as f64 - CLOCK_HEIGHT) / 2.0),
                NSSize::new(body.width as f64, CLOCK_HEIGHT),
            ));
        }
        self.setNeedsDisplay(true);
    }
}

pub fn run() {
    let mtm = MainThreadMarker::new().expect("main thread");
    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon, no menu bar of its own.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let screens = NSScreen::screens(mtm);
    // Cocoa's y grows up from the first screen's bottom.
    let Some(first) = screens.iter().next() else {
        return;
    };
    let top = first.frame().size.height;
    // Only where there is no notch (#18).
    let Some(screen) = screens.iter().find(|s| s.safeAreaInsets().top == 0.0) else {
        eprintln!("island: every screen has a notch; nothing to do here");
        return;
    };
    let f = screen.frame();
    let monitor = Rect {
        x: f.origin.x as f32,
        y: (top - f.origin.y - f.size.height) as f32,
        width: f.size.width as f32,
        height: f.size.height as f32,
    };
    // Room for the open body's wobble, like the Windows app.
    let r = shape::window_rect(
        PLACEMENT,
        monitor,
        OPEN.0 + 2.0 * spring::ROOM_WIDTH,
        OPEN.1 + spring::ROOM_HEIGHT,
        EAR,
        0.0,
    );
    let frame = NSRect::new(
        NSPoint::new(r.x as f64, top - (r.y + r.height) as f64),
        NSSize::new(r.width as f64, r.height as f64),
    );

    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        frame,
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        false,
    );
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(false);
    panel.setHidesOnDeactivate(false);
    panel.setLevel(LEVEL);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    let view = IslandView::new(mtm, NSRect::new(NSPoint::new(0.0, 0.0), frame.size));
    panel.setContentView(Some(&view));
    panel.orderFrontRegardless();

    app.run();
}
