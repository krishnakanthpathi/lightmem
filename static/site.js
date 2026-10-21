/* Progressive enhancements for shared UI. The page renders before this module. */
export function announce(message) {
  let status = document.getElementById('uiStatus');
  if (!status) {
    status = document.createElement('div');
    status.id = 'uiStatus';
    status.className = 'sr-only';
    status.setAttribute('role', 'status');
    document.body.append(status);
  }
  status.textContent = message;
}

document.querySelectorAll('.copy').forEach(button => {
  const label = button.textContent;
  const code = button.closest('.code')?.querySelector('pre');
  button.type = 'button';
  button.setAttribute('aria-label', 'Copy ' + (button.closest('.code')?.querySelector('.code-head > span')?.textContent || 'code'));
  let reset;
  button.addEventListener('click', async () => {
    const text = button.dataset.copy ?? code?.textContent.replace(/^\$ /gm, '').trim();
    if (!text) return;
    clearTimeout(reset);
    try {
      await navigator.clipboard.writeText(text);
      button.textContent = 'Copied';
      announce('Code copied to clipboard');
    } catch {
      if (code) {
        const range = document.createRange();
        range.selectNodeContents(code);
        const selection = window.getSelection();
        selection.removeAllRanges();
        selection.addRange(range);
      }
      button.textContent = 'Select & copy';
      announce('Clipboard unavailable. Code selected; press Control or Command C to copy.');
    }
    reset = setTimeout(() => { button.textContent = label; }, 1800);
  });
});

// Keep native anchor navigation working with a simple local static server, too.
if (location.hostname === 'localhost' || location.hostname === '127.0.0.1') {
  document.querySelectorAll('a[href^="/docs"]').forEach(link => {
    link.setAttribute('href', link.getAttribute('href').replace(/^\/docs(?=#|$)/, '/docs.html'));
  });
}

if (!['localhost', '127.0.0.1', '[::1]'].includes(location.hostname)) {
  window.va ||= function () { (window.vaq ||= []).push(arguments); };
  window.si ||= function () { (window.siq ||= []).push(arguments); };
  for (const src of ['/_vercel/insights/script.js', '/_vercel/speed-insights/script.js']) {
    const script = document.createElement('script');
    script.src = src;
    script.defer = true;
    document.head.append(script);
  }
}
