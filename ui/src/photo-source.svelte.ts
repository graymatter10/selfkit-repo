// Shared "add photo" source choice + "Save to gallery" for Agenda, Hisab and
// Chrono. Docs: docs/components/photo-source.md.
//
// Android (window.SelfkitPhotos bridge present, see SelfkitPhotos.kt in each
// app): "add photo" opens a small bottom sheet (Take photo / Choose from
// gallery / Cancel). Both choices click a hidden <input type=file> the app
// owns: the camera one has capture="environment", which Tauri's file chooser
// turns into ACTION_IMAGE_CAPTURE writing to the app's private external files
// dir (never the phone's gallery). The app imports the File with its usual
// resize/WebP path, then calls cleanCaptures() to delete the temporary capture.
// Desktop (no bridge): the gallery picker opens straight away, as before.
//
// Imported by the apps through a relative path (ui/package.json exports only
// the index), e.g. apps/<app>/src/lib/photo-source.ts re-exports this file.
import { showToast } from "./toast.svelte";

type Bridge = {
  hasCamera(): boolean;
  cleanCaptures(): number;
  saveToGallery(b64: string, mime: string, name: string): string;
};
const bridge = (): Bridge | undefined => (window as unknown as { SelfkitPhotos?: Bridge }).SelfkitPhotos;

export type PhotoSources = { gallery: () => void; camera: () => void; cancel?: () => void };

/** Sheet state (rendered by PhotoSourceSheet.svelte, mounted once per app). */
export const photoSource = $state<{ open: boolean }>({ open: false });
let pending: PhotoSources | null = null;

/** Ask where the photo comes from (Android) or open the gallery picker (desktop). */
export function askPhotoSource(src: PhotoSources) {
  const b = bridge();
  let cam = false;
  try {
    cam = !!b?.hasCamera();
  } catch {
    cam = false;
  }
  if (!cam) return src.gallery();
  pending = src;
  photoSource.open = true;
}

/** Runs inside the sheet button's tap, so the input click keeps user activation. */
export function choosePhotoSource(which: "camera" | "gallery") {
  const src = pending;
  pending = null;
  closePhotoSource();
  if (!src) return;
  if (which === "camera") {
    cleanCaptures();
    src.camera();
  } else src.gallery();
}

/** Close the sheet (Back, Escape, Cancel, scrim). False when it was not open. */
export function closePhotoSource(): boolean {
  if (!photoSource.open) return false;
  photoSource.open = false;
  const src = pending;
  pending = null;
  src?.cancel?.();
  return true;
}

/** Delete temporary camera captures (call after importing picked files). */
export function cleanCaptures() {
  try {
    bridge()?.cleanCaptures();
  } catch {
    /* desktop or old bridge */
  }
}

function b64(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf);
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}

const EXT: Record<string, string> = { "image/webp": "webp", "image/jpeg": "jpg", "image/png": "png" };

/**
 * "Save to gallery" from a photo viewer, with a toast either way.
 * Android: MediaStore → Pictures/Selfkit/<App>. Desktop: `desktop()` (the
 * app's Rust photo_export command, copies into ~/Pictures/Selfkit/<App>)
 * returns the saved path.
 * `data`: a data: URL (Agenda, Chrono) or raw bytes + mime (Hisab).
 */
export async function saveToGallery(
  appName: string,
  data: () => Promise<{ dataUrl?: string; bytes?: ArrayBuffer; mime?: string }>,
  desktop: () => Promise<string>,
): Promise<boolean> {
  try {
    const b = bridge();
    if (!b) {
      const path = await desktop();
      showToast(`Saved to ${path.replace(/^\/home\/[^/]+/, "~")}`);
      return true;
    }
    const d = await data();
    let mime = d.mime ?? "image/webp", body = "";
    if (d.dataUrl) {
      const m = /^data:([^;,]+);base64,(.*)$/s.exec(d.dataUrl);
      if (!m) throw new Error("bad image");
      mime = m[1];
      body = m[2];
    } else if (d.bytes) body = b64(d.bytes);
    const t = new Date(), p = (n: number) => String(n).padStart(2, "0");
    const name = `${appName}_${t.getFullYear()}${p(t.getMonth() + 1)}${p(t.getDate())}_${p(t.getHours())}${p(t.getMinutes())}${p(t.getSeconds())}.${EXT[mime] ?? "jpg"}`;
    const where = b.saveToGallery(body, mime, name);
    if (!where) throw new Error("no space or no permission");
    showToast(`Saved to gallery · ${where.replace(/\/[^/]+$/, "")}`);
    return true;
  } catch (e) {
    showToast(`Could not save the photo: ${String(e).replace(/^Error:\s*/, "")}`);
    return false;
  }
}
