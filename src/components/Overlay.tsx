import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { SideMenu } from "./SideMenu";
import "../overlay.css";

export function Overlay() {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const unlisteners = Promise.all([
      listen("overlay-open", () => setOpen(true)),
      listen("overlay-close", () => setOpen(false)),
    ]);
    return () => {
      unlisteners.then((fns) => fns.forEach((fn) => fn()));
    };
  }, []);

  return (
    <SideMenu
      open={open}
      overlay
      onClose={() => {
        setOpen(false);
        invoke("hide_overlay").catch((err) => console.error(err));
      }}
    />
  );
}
