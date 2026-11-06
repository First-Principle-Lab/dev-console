<script lang="ts">
  import {
    ArrowLeft, Check, Copy, Eye, Inbox, KeyRound, List, LockKeyhole,
    LockKeyholeOpen, Pencil, Pin, Play, Plus, RefreshCw, Search,
    Settings, Shield, Terminal, Trash2, TriangleAlert
  } from '@lucide/svelte';
  import type { IconName } from '../lib/icons';

  let { name, size = 14 }: { name: IconName; size?: number } = $props();
  const icons = {
    command: Terminal,
    collection: List,
    inbox: Inbox,
    secret: KeyRound,
    lock: LockKeyhole,
    unlock: LockKeyholeOpen,
    copy: Copy,
    reveal: Eye,
    edit: Pencil,
    pin: Pin,
    run: Play,
    trash: Trash2,
    search: Search,
    plus: Plus,
    back: ArrowLeft,
    alert: TriangleAlert,
    check: Check,
    shield: Shield,
    gear: Settings,
    update: RefreshCw
  } satisfies Record<IconName, typeof Terminal>;
  let Component = $derived(icons[name]);
</script>

<Component {size} strokeWidth={1.75} aria-hidden="true" focusable="false" />
