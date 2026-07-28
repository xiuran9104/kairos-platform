/**
 * Render markdown-like content to HTML for chat message display.
 */
export function renderContent(content: string, _type: string, role?: string): string {
  if (!content) return '';
  let html = content
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
  html = html.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
  html = html.replace(
    /`([^`]+)`/g,
    '<code style="background:var(--ant-color-fill-secondary);padding:1px 4px;border-radius:3px;font-size:12px">$1</code>',
  );
  const linkColor =
    role === 'user'
      ? 'color:#fff;text-decoration:underline'
      : 'color:var(--ant-color-primary)';
  html = html.replace(
    /https?:\/\/[^\s<>"]+/g,
    `<a href="$&" target="_blank" rel="noopener" style="${linkColor}">$&</a>`,
  );
  html = html.replace(/\n/g, '<br>');
  return html;
}
