// Line icons (24×24, stroke = currentColor) for categories and the UI.
export const CAT_ICONS: Record<string, string> = {
  dot: '<circle cx="12" cy="12" r="3.2"/>',
  food: '<path d="M7 3v8a2 2 0 0 0 2 2v8M11 3v8M7 7h4M16 21V3c2.5 1.5 3.5 4 3.5 7s-1.5 4-3.5 4"/>',
  coffee: '<path d="M4 9h12v5a5 5 0 0 1-5 5H9a5 5 0 0 1-5-5zM16 10h1.5a2.5 2.5 0 0 1 0 5H16M8 3v3M12 3v3"/>',
  cart: '<path d="M3 4h2l2.2 10.2a1.5 1.5 0 0 0 1.5 1.2h8.6a1.5 1.5 0 0 0 1.5-1.1L21 8H6.2"/><circle cx="9.5" cy="19.5" r="1.3"/><circle cx="17" cy="19.5" r="1.3"/>',
  car: '<path d="M5 16V11l2-5h10l2 5v5M3 16h18v3h-3v-3M6 16v3H3"/><circle cx="7.5" cy="13" r=".6"/><circle cx="16.5" cy="13" r=".6"/><path d="M5 11h14"/>',
  bus: '<rect x="5" y="3" width="14" height="15" rx="3"/><path d="M5 11h14M8 18v3M16 18v3"/><circle cx="8.5" cy="14.5" r=".6"/><circle cx="15.5" cy="14.5" r=".6"/>',
  fuel: '<path d="M4 21V5a2 2 0 0 1 2-2h6a2 2 0 0 1 2 2v16M3 21h12M4 10h10M14 8l3 3v6a1.5 1.5 0 0 0 3 0V9l-3-3"/>',
  plane: '<path d="M10.5 13.5 4 12l1-2 7 .5L16 5c1-1 2.5-1 3 0s0 2-1 3l-5.5 4 .5 7-2 1-1.5-6.5z"/>',
  home: '<path d="M4 11 12 4l8 7M6 9.5V20h12V9.5M10 20v-5h4v5"/>',
  bolt: '<path d="M13 3 5 13h6l-1 8 8-10h-6z"/>',
  wifi: '<path d="M2.5 9a14 14 0 0 1 19 0M5.5 12.5a9.5 9.5 0 0 1 13 0M8.5 16a5 5 0 0 1 7 0"/><circle cx="12" cy="19" r=".8"/>',
  phone: '<rect x="7" y="2.5" width="10" height="19" rx="2.5"/><path d="M11 18.5h2"/>',
  bag: '<path d="M5 8h14l-1 12H6zM9 8V6a3 3 0 0 1 6 0v2"/>',
  shirt: '<path d="M8 3 3 6l2 4 2-1v12h10V9l2 1 2-4-5-3a4 4 0 0 1-8 0z"/>',
  heart: '<path d="M12 20s-7-4.4-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 10c0 5.6-7 10-7 10z"/>',
  pill: '<rect x="3" y="8.5" width="18" height="7" rx="3.5" transform="rotate(-45 12 12)"/><path d="m9.5 9.5 5 5"/>',
  fitness: '<path d="M6 7v10M18 7v10M3 10v4M21 10v4M6 12h12"/>',
  star: '<path d="m12 3.5 2.6 5.3 5.8.8-4.2 4.1 1 5.8L12 16.8l-5.2 2.7 1-5.8-4.2-4.1 5.8-.8z"/>',
  film: '<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M7 5v14M17 5v14M3 9.5h4M3 14.5h4M17 9.5h4M17 14.5h4"/>',
  game: '<path d="M7 9h10a4 4 0 0 1 3.9 4.9l-.6 2.6a2 2 0 0 1-3.4.9L15 15H9l-1.9 2.4a2 2 0 0 1-3.4-.9l-.6-2.6A4 4 0 0 1 7 9zM8 11v3M6.5 12.5h3"/><circle cx="15.5" cy="12" r=".6"/><circle cx="17" cy="13.5" r=".6"/>',
  book: '<path d="M4 5a2 2 0 0 1 2-2h13v16H6a2 2 0 0 0-2 2zM4 19V5M19 19v2H6"/>',
  school: '<path d="m2 9 10-5 10 5-10 5zM6 11v5c0 1.5 3 3 6 3s6-1.5 6-3v-5M22 9v6"/>',
  gift: '<rect x="3.5" y="8" width="17" height="4" rx="1"/><path d="M5 12v8h14v-8M12 8v12M12 8c-1-3-5-4-5-1.5S12 8 12 8zm0 0c1-3 5-4 5-1.5S12 8 12 8z"/>',
  baby: '<circle cx="12" cy="8" r="4"/><path d="M6 21c0-4 2.7-7 6-7s6 3 6 7M10.5 8h.01M13.5 8h.01"/>',
  paw: '<circle cx="8" cy="7" r="1.7"/><circle cx="16" cy="7" r="1.7"/><circle cx="5" cy="12" r="1.6"/><circle cx="19" cy="12" r="1.6"/><path d="M12 12c-3 0-5 4-5 6s2 2 5 1 5 1 5-1-2-6-5-6z"/>',
  tools: '<path d="M14.5 6.5a4 4 0 0 0 5 5L12 19a2.1 2.1 0 0 1-3-3l7.5-7.5zM4 4l4 4M3 7l4-4"/>',
  briefcase: '<rect x="3" y="7" width="18" height="13" rx="2"/><path d="M8 7V5a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M3 12.5h18"/>',
  chart: '<path d="M4 20V4M4 20h16M8 16v-4M12 16V8M16 16v-6"/>',
  cash: '<rect x="2.5" y="6" width="19" height="12" rx="2"/><circle cx="12" cy="12" r="2.5"/><path d="M6 9.5v5M18 9.5v5"/>',
  bank: '<path d="m3 9 9-5 9 5M5 9v9M19 9v9M9.5 9v9M14.5 9v9M3 20h18"/>',
  wallet: '<path d="M4 7.5V18a2 2 0 0 0 2 2h13a1 1 0 0 0 1-1v-3M4 7.5A2.5 2.5 0 0 1 6.5 5H17v2.5M4 7.5h15a1 1 0 0 1 1 1V11"/><path d="M15.5 11H21v5h-5.5a2.5 2.5 0 0 1 0-5z"/>',
  card: '<rect x="2.5" y="5" width="19" height="14" rx="2"/><path d="M2.5 10h19M6 15h4"/>',
  repeat: '<path d="M4 11V9a3 3 0 0 1 3-3h13M17 3l3 3-3 3M20 13v2a3 3 0 0 1-3 3H4M7 21l-3-3 3-3"/>',
  target: '<circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="4.5"/><circle cx="12" cy="12" r=".8"/>',
  tag: '<path d="M3 12V4h8l10 10-8 8z"/><circle cx="7.5" cy="8.5" r="1.2"/>',
  export: '<path d="M12 3v12M7 8l5-5 5 5M5 15v4a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-4"/>',
  sms: '<path d="M4 5h16v11H9l-5 4z"/>',
  people: '<circle cx="9" cy="8" r="3.2"/><path d="M3 20c0-3.3 2.7-6 6-6s6 2.7 6 6M16 4.5a3.2 3.2 0 0 1 0 6.3M18 14.3c1.8.8 3 2.7 3 5.7"/>',
};

export const CAT_ICON_NAMES = Object.keys(CAT_ICONS).filter((k) => !["cash", "bank", "wallet", "card", "repeat", "people", "target", "tag", "export", "sms"].includes(k)).concat(["cash", "people"]);

export const COLORS = ["#d9822b", "#c4a43b", "#5a9e5f", "#2e7a4d", "#3b9e9e", "#3b82c4", "#5b6cd1", "#a05ac4", "#e0719c", "#d1495b", "#8c6a4f", "#8a8a8e"];

export const ACCOUNT_ICON: Record<string, string> = { cash: "cash", bank: "bank", wallet: "phone", card: "card", other: "wallet" };
