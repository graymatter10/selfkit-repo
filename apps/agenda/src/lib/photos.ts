// Photo attach: scale to document quality (longest side ≤ 2400 px) and encode
// as WebP (JPEG where the webview cannot encode WebP), then store through Rust.
// Display: photos come back as data: URLs, cached in memory.
import { api } from "./api";

export const MAX_SIDE = 2400;
const QUALITY = 0.85;

async function decode(file: Blob): Promise<{ img: CanvasImageSource; w: number; h: number; close?: () => void }> {
  if ("createImageBitmap" in window) {
    try {
      const b = await createImageBitmap(file, { imageOrientation: "from-image" } as ImageBitmapOptions);
      return { img: b, w: b.width, h: b.height, close: () => b.close() };
    } catch {
      /* fall back to <img> */
    }
  }
  const url = URL.createObjectURL(file);
  try {
    const img = new Image();
    img.src = url;
    await img.decode();
    return { img, w: img.naturalWidth, h: img.naturalHeight };
  } finally {
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
}

const toBlob = (c: HTMLCanvasElement, type: string) => new Promise<Blob | null>((r) => c.toBlob(r, type, QUALITY));

function b64(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf);
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}

/** Scale, encode and save one picked image; returns the photo id. */
export async function attachPhoto(file: File): Promise<string> {
  const { img, w, h, close } = await decode(file);
  const k = Math.min(1, MAX_SIDE / Math.max(w, h));
  const cw = Math.max(1, Math.round(w * k)), ch = Math.max(1, Math.round(h * k));
  const c = document.createElement("canvas");
  c.width = cw;
  c.height = ch;
  const ctx = c.getContext("2d")!;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(img, 0, 0, cw, ch);
  close?.();
  let blob = await toBlob(c, "image/webp");
  if (!blob || blob.type !== "image/webp") blob = await toBlob(c, "image/jpeg");
  if (!blob) throw new Error("Could not read that image.");
  c.width = c.height = 0; // free the canvas memory right away
  const id = await api.photoSave(b64(await blob.arrayBuffer()), blob.type, cw, ch);
  return id;
}

const cache = new Map<string, Promise<string>>();
export function photoUrl(id: string): Promise<string> {
  if (!cache.has(id)) {
    const p = api.photoGet(id);
    p.catch(() => cache.delete(id));
    cache.set(id, p);
  }
  return cache.get(id)!;
}
