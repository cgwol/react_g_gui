import "./App.css";
import { useEffect, useState, useRef, useCallback } from "react";
import { Routes, Route, useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { SideMenu } from "./components/SideMenu";
import { Overlay } from "./components/Overlay";
import { setInputMode, useInputMode, getPad } from "./input-mode";
import { Games } from "./pages/Games";
import { Apps } from "./pages/Apps";
import { Settings } from "./pages/Settings";
import { getDeadzone, applyWindowMode, getWindowMode } from "./settings";
import { useSystemSounds } from "./sound";

const TILES = ["Games", "Apps", "Settings"];

function isOverlayWindow() {
  try {
    return getCurrentWindow().label === "overlay";
  } catch {
    return false;
  }
}

function useCursorMode() {
  useEffect(() => {
    const hide = () => document.body.classList.add("cursor-hidden");
    const show = () => document.body.classList.remove("cursor-hidden");

    const onKey = () => {
      setInputMode("keyboard");
      hide();
    };
    const onMouse = (e: MouseEvent) => {
      if (Math.abs(e.movementX) + Math.abs(e.movementY) > 2) {
        setInputMode("mouse");
        show();
      }
    };
    const onMouseDown = () => {
      setInputMode("mouse");
      show();
    };
    const onWheel = () => {
      setInputMode("mouse");
      show();
    };

    window.addEventListener("keydown", onKey);
    window.addEventListener("mousemove", onMouse);
    window.addEventListener("mousedown", onMouseDown);
    window.addEventListener("wheel", onWheel);

    let alive = true;
    let raf = 0;
    let prev = "";

    const tick = () => {
      if (!alive) return;
      const pad = getPad();
      if (pad) {
        const x = pad.axes[0] ?? 0;
        const y = pad.axes[1] ?? 0;
        const stick = Math.abs(x) > 0.6 || Math.abs(y) > 0.6;
        const pressed = pad.buttons.some((b) => b.pressed);
        const now = `${stick}:${pressed}`;
        if (now !== prev && (stick || pressed)) {
          setInputMode("gamepad", pad.id);
          hide();
        }
        prev = now;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);

    return () => {
      alive = false;
      cancelAnimationFrame(raf);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mousemove", onMouse);
      window.removeEventListener("mousedown", onMouseDown);
      window.removeEventListener("wheel", onWheel);
    };
  }, []);
}

function ShellNav() {
  const navigate = useNavigate();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<string>("shell-navigate", (event) => {
      navigate(event.payload || "/");
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [navigate]);
  return null;
}

function Home() {
  const [focused, setFocused] = useState(0);
  const focusedRef = useRef(focused);
  focusedRef.current = focused;
  const navigate = useNavigate();
  const [menuOpen, setMenuOpen] = useState(false);
  const menuOpenRef = useRef(menuOpen);
  menuOpenRef.current = menuOpen;
  const { mode, padKind } = useInputMode();
  const closeMenu = useCallback(() => setMenuOpen(false), []);
  const menuIcon =
    mode === "keyboard"
      ? "Esc"
      : mode === "gamepad" && padKind === "playstation"
        ? "OPTIONS"
        : mode === "gamepad"
          ? "\u2630"
          : "";

  const open = () => navigate("/" + TILES[focusedRef.current].toLowerCase());

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (!menuOpenRef.current) setMenuOpen(true);
        return;
      }
      if (menuOpenRef.current) return;
      if (e.key === "ArrowLeft") {
        setFocused((f) => (f - 1 + TILES.length) % TILES.length);
      }
      if (e.key === "ArrowRight") {
        setFocused((f) => (f + 1) % TILES.length);
      }
      if (e.key === "Enter" || e.key === " ") open();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    let alive = true;
    let raf = 0;
    let prevCross = false;
    let prevMenu = false;
    let armed = false;
    let holdDir: "left" | "right" | null = null;
    let holdSince = 0;
    let lastFire = 0;
    const HOLD_DELAY = 420;
    const REPEAT = 180;

    const move = (dir: "left" | "right") => {
      setFocused((f) =>
        dir === "left"
          ? (f - 1 + TILES.length) % TILES.length
          : (f + 1) % TILES.length,
      );
    };

    const tick = () => {
      if (!alive) return;
      const pad = getPad();

      if (pad) {
        const zone = getDeadzone();
        const x = pad.axes[0] ?? 0;
        const left = x < -zone || !!pad.buttons[14]?.pressed;
        const right = x > zone || !!pad.buttons[15]?.pressed;
        const cross = !!pad.buttons[0]?.pressed;
        const circle = !!pad.buttons[1]?.pressed;
        const ps = !!pad.buttons[16]?.pressed;
        const start = !!pad.buttons[9]?.pressed;
        const now = performance.now();

        if (!menuOpenRef.current) {
          const active =
            left && !right ? "left" : right && !left ? "right" : null;

          if (active !== holdDir) {
            holdDir = active;
            holdSince = now;
            lastFire = now;
            if (active) move(active);
          } else if (
            active &&
            now - holdSince >= HOLD_DELAY &&
            now - lastFire >= REPEAT
          ) {
            lastFire = now;
            move(active);
          }

          if (cross && !prevCross) open();
        }

        if (!circle && !ps && !start) armed = true;
        if (
          armed &&
          !menuOpenRef.current &&
          (circle || ps || start) &&
          !prevMenu
        ) {
          setMenuOpen(true);
        }
        prevCross = cross;
        prevMenu = circle || ps || start;
      } else {
        prevCross = false;
        prevMenu = false;
        holdDir = null;
      }

      raf = requestAnimationFrame(tick);
    };

    raf = requestAnimationFrame(tick);
    return () => {
      alive = false;
      cancelAnimationFrame(raf);
    };
  }, []);

  return (
    <div className="shell">
      <header className="topbar">
        <span className="brand" onClick={() => setMenuOpen(true)}>
          {menuIcon && <span className="keycap">{menuIcon}</span>}
          Home
        </span>
        <span className="clock">
          {new Date().toLocaleTimeString([], {
            hour: "numeric",
            minute: "2-digit",
          })}
        </span>
      </header>
      <div className="row">
        {TILES.map((label, i) => (
          <button
            key={label}
            type="button"
            className={i === focused ? "tile focused" : "tile"}
            onMouseEnter={() => setFocused(i)}
            onClick={() => navigate("/" + label.toLowerCase())}
          >
            <span className="tile-label">{label}</span>
          </button>
        ))}
      </div>
      <SideMenu open={menuOpen} onClose={closeMenu} />
    </div>
  );
}

function Shell() {
  useCursorMode();
  useSystemSounds();
  useEffect(() => {
    void applyWindowMode(getWindowMode());
  }, []);
  return (
    <>
      <ShellNav />
      <Routes>
        <Route path="/" element={<Home />} />
        <Route path="/games" element={<Games />} />
        <Route path="/apps" element={<Apps />} />
        <Route path="/settings" element={<Settings />} />
      </Routes>
    </>
  );
}

export default function App() {
  if (isOverlayWindow()) return <Overlay />;
  return <Shell />;
}
