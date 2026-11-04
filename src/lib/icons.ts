export type IconName =
  | 'command'
  | 'collection'
  | 'inbox'
  | 'secret'
  | 'lock'
  | 'unlock'
  | 'copy'
  | 'reveal'
  | 'edit'
  | 'pin'
  | 'run'
  | 'trash'
  | 'search'
  | 'plus'
  | 'back'
  | 'alert'
  | 'check'
  | 'shield'
  | 'gear'
  | 'update';

/** Single source for the inline icon set (20x20 viewBox, `fill: currentColor`). */
export const ICON_PATHS: Record<IconName, string> = {
  command: 'M3 4.5h14v11H3z M3 7.5h14 M6 10l2 2-2 2 M10 14h3',
  collection: 'M3 4h3 M9 4h8 M3 10h3 M9 10h8 M3 16h3 M9 16h8',
  inbox: 'M3 4h14l2 10v4H1v-4L3 4z M1 14h5l1.5 2h5L14 14h5',
  secret: 'M9 2a5 5 0 0 0-5 5v2H2v9h16V9h-2V7a5 5 0 0 0-5-5Zm0 2a3 3 0 0 1 3 3v2H6V7a3 3 0 0 1 3-3Z',
  lock: 'M11 2a4 4 0 0 0-4 4v2H4v10h14V8h-3V6a4 4 0 0 0-4-4Zm0 2a2 2 0 0 1 2 2v2H9V6a2 2 0 0 1 2-2Z',
  unlock: 'M6 8V6a4 4 0 0 1 7.7-1.5l-1.8.9A2 2 0 0 0 8 6v2h7v10H2V8h4Zm-2 4v4h10v-4H4Z',
  copy: 'M8 2h10v12h-3V5H8V2ZM5 6h9v14H5V6Zm2 2v10h5V8H7Z',
  reveal:
    'M10 5C5 5 1.7 8.5.5 10c1.2 1.5 4.5 5 9.5 5s8.3-3.5 9.5-5C18.3 8.5 15 5 10 5Zm0 3a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5Z',
  edit: 'M2 14.5 11.6 5l3.4 3.4L5.4 17.9 2 18l-.1-3.4ZM13.2 3.4l1.4-1.4 3.4 3.4-1.4 1.4-3.4-3.4Z',
  pin: 'M9 1v5L6 9H4v2l5 1 2 6 2-6 5-1V9h-2l-3-3V1H9Z',
  run: 'M5 2l14 8-14 8V2Z',
  trash: 'M7 2h6l1 2h4v2H2V4h4l1-2ZM4 8h12l-1 12H5L4 8Z',
  search: 'M9 1a8 8 0 1 0 4.9 14.3l4.4 4.4 1.4-1.4-4.4-4.4A8 8 0 0 0 9 1Zm0 2a6 6 0 1 1 0 12A6 6 0 0 1 9 3Z',
  plus: 'M9 2v6H3v2h6v6h2v-6h6V8h-6V2H9Z',
  back: 'M10 3 2 10l8 7 1.4-1.4L5.7 11H17V9H5.7l5.7-4.6L10 3Z',
  alert: 'M9 1 0 17h18L9 1Zm-1 5h2v6H8V6Zm0 7h2v2H8v-2Z',
  check: 'M15.9 4 7 12.9 4.1 10 2.7 11.4 7 15.7l10.3-10.3L15.9 4Z',
  shield: 'M9 1 2 4v6c0 4.4 3 7.7 7 9 4-1.3 7-4.6 7-9V4L9 1Zm0 2.2 5 2.2V10c0 3.2-2 5.8-5 6.9-3-1.1-5-3.7-5-6.9V5.4l5-2.2Z',
  gear: 'M9 6a3 3 0 1 0 0 6 3 3 0 0 0 0-6Zm7.4 4 .1-1-.1-1 1.7-1.4-1.6-2.8-2.1.8a6.9 6.9 0 0 0-1.7-1L12.4 1H7.6L7.2 3.6c-.6.2-1.2.5-1.7 1l-2.1-.8L1.8 6.6 3.5 8a6.9 6.9 0 0 0 0 2L1.8 11.4l1.6 2.8 2.1-.8c.5.4 1.1.8 1.7 1l.4 2.6h4.8l.4-2.6c.6-.2 1.2-.5 1.7-1l2.1.8 1.6-2.8L16.4 10Z',
  update: 'M10 2a8 8 0 0 1 7.7 6h-2.1A6 6 0 0 0 5 5.7V8H3V2h2v1.4A8 8 0 0 1 10 2Zm5 10h2v6h-2v-1.4A8 8 0 0 1 2.3 12h2.1A6 6 0 0 0 15 14.3V12Z'
};
