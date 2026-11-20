<script lang="ts">
  // Stroke icons from the approved design (24×24, currentColor).
  const ICON: Record<string, string> = {
    today: '<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/>',
    timeline: '<path d="M4 7h9M4 12h14M4 17h6"/><circle cx="16" cy="7" r="1.6"/><circle cx="13" cy="17" r="1.6"/>',
    trends: '<path d="M5 19V11M10 19V6M15 19v-5M20 19V9"/>',
    report: '<rect x="5" y="4" width="14" height="16" rx="2"/><path d="M9 9h6M9 13h6M9 17h3"/>',
    gear: '<circle cx="12" cy="12" r="3"/><path d="M12 3v2.2M12 18.8V21M3 12h2.2M18.8 12H21M5.6 5.6l1.6 1.6M16.8 16.8l1.6 1.6M5.6 18.4l1.6-1.6M16.8 7.2l1.6-1.6"/>',
    left: '<path d="M15 6l-6 6 6 6"/>', right: '<path d="M9 6l6 6-6 6"/>', close: '<path d="M7 7l10 10M17 7L7 17"/>', back: '<path d="M19 12H5M11 6l-6 6 6 6"/>',
    plus: '<path d="M12 5v14M5 12h14"/>', pause: '<path d="M9 6v12M15 6v12"/>', play: '<path d="M8 5.5v13l10-6.5z"/>',
    phone: '<rect x="7" y="3" width="10" height="18" rx="2.5"/><path d="M11 18h2"/>',
    pc: '<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M9 20h6M12 16v4"/>',
    globe: '<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.5 2.6 2.5 14.4 0 17M12 3.5c-2.5 2.6-2.5 14.4 0 17"/>',
    search: '<circle cx="11" cy="11" r="6.5"/><path d="M20 20l-4.2-4.2"/>', down: '<path d="M7 10l5 5 5-5"/>',
    shield: '<path d="M12 3l7 3v5c0 4.5-3 8-7 10-4-2-7-5.5-7-10V6z"/>', bell: '<path d="M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15z"/><path d="M10 20.5a2 2 0 0 0 4 0"/>',
    check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>', no: '<circle cx="12" cy="12" r="8.5"/><path d="M6 18L18 6"/>',
    lock: '<rect x="5" y="10.5" width="14" height="10" rx="2"/><path d="M8 10.5V8a4 4 0 0 1 8 0v2.5"/>',
    chart: '<path d="M4 20h16"/><rect x="6" y="11" width="3" height="6" rx="1"/><rect x="11" y="6" width="3" height="11" rx="1"/><rect x="16" y="9" width="3" height="8" rx="1"/>',
    pen: '<path d="M4 20h4L19 9l-4-4L4 16z"/><path d="M13.5 6.5l4 4"/>', camera: '<path d="M4 8h3l2-2.5h6L17 8h3v11H4z"/><circle cx="12" cy="13" r="3.5"/>',
    image: '<rect x="3.5" y="5" width="17" height="14" rx="2"/><circle cx="9" cy="10" r="1.6"/><path d="M20.5 16l-5-5L6 19"/>',
    ext: '<path d="M14 5h5v5M19 5l-8 8M17 14v4a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1h4"/>',
    limit: '<path d="M4 18a8 8 0 1 1 16 0"/><path d="M12 18l4-5"/>', clock: '<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/>',
    sites: '<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.5 2.6 2.5 14.4 0 17M12 3.5c-2.5 2.6-2.5 14.4 0 17"/>',
    trash: '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/>',
  };
  let { name, size = 18, stroke = 1.8 }: { name: string; size?: number; stroke?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width={stroke} stroke-linecap="round" stroke-linejoin="round" style="flex:none" aria-hidden="true">{@html ICON[name] ?? ""}</svg>
