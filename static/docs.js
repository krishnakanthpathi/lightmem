import { announce } from './site.js';

const search = document.getElementById('docSearch');
const results = document.getElementById('searchResults');
const menu = document.querySelector('.docs-navigation');
const sections = [...document.querySelectorAll('.searchable')];
const navLinks = [...document.querySelectorAll('.side-nav a')];
const commands = [...document.querySelectorAll('.command')];
const normalize = value => value.toLowerCase().replace(/[^a-z0-9_.-]+/g, ' ').trim();
const mobile = matchMedia('(max-width: 820px)');

function syncMenu() { menu.open = !mobile.matches; }
syncMenu();
mobile.addEventListener('change', syncMenu);

// Cache text once, on demand. Typing never measures or hides the document.
let index;
function getIndex() {
  if (index) return index;
  index = [
    ...commands.map(element => ({
      id: element.id,
      title: 'lmem ' + element.querySelector('.command-name').textContent.trim().replace(/^\d+\s*/, ''),
      description: element.querySelector('.command-desc').textContent,
      text: normalize(element.textContent + ' ' + element.dataset.search),
      command: true
    })),
    ...sections.map(element => ({
      id: element.id,
      title: element.querySelector('h1, h2').textContent,
      description: element.querySelector('.lead, :scope > p')?.textContent || '',
      text: normalize(element.dataset.search + ' ' + element.textContent),
      command: false
    }))
  ];
  return index;
}
function closeSearch() {
  results.hidden = true;
  search.setAttribute('aria-expanded', 'false');
}
function searchDocs() {
  const query = normalize(search.value);
  if (!query) { closeSearch(); return; }
  const terms = query.split(/\s+/);
  const matches = getIndex().filter(item => terms.every(term => item.text.includes(term)));
  matches.sort((a, b) => {
    const score = item => (normalize(item.title).includes(query) ? 2 : 0) + (item.command ? 1 : 0);
    return score(b) - score(a);
  });
  results.replaceChildren();
  matches.slice(0, 8).forEach(item => {
    const link = document.createElement('a');
    link.className = 'search-result';
    link.href = '#' + item.id;
    const title = document.createElement('strong');
    title.textContent = item.title;
    const description = document.createElement('span');
    description.textContent = item.description.slice(0, 140);
    link.append(title, description);
    results.append(link);
  });
  if (!matches.length) {
    const message = document.createElement('div');
    message.className = 'search-message';
    message.textContent = 'No results. Try “recall”, “Python”, or “configuration”.';
    results.append(message);
  }
  results.hidden = false;
  search.setAttribute('aria-expanded', 'true');
  announce(matches.length ? `${Math.min(matches.length, 8)} results shown for ${search.value}` : 'No documentation matched');
}
search.addEventListener('input', searchDocs);
search.addEventListener('focus', () => { if (search.value.trim()) searchDocs(); });
search.addEventListener('keydown', event => {
  if (event.key === 'ArrowDown' && !results.hidden) {
    event.preventDefault();
    results.querySelector('a')?.focus();
  }
  if (event.key === 'Enter' && !results.hidden) {
    event.preventDefault();
    results.querySelector('a')?.click();
  }
});
results.addEventListener('keydown', event => {
  const links = [...results.querySelectorAll('a')];
  const current = links.indexOf(document.activeElement);
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    const next = event.key === 'ArrowDown' ? current + 1 : current - 1;
    if (next < 0) search.focus();
    else links[Math.min(next, links.length - 1)]?.focus();
  }
});
document.addEventListener('click', event => {
  if (!event.target.closest('.search-wrap')) closeSearch();
});
document.addEventListener('focusin', event => {
  if (!event.target.closest('.search-wrap')) closeSearch();
});
document.addEventListener('keydown', event => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault(); search.focus(); search.select();
  }
  if (event.key === 'Escape') {
    if (!results.hidden) { closeSearch(); search.focus(); }
    else if (document.activeElement === search) { search.value = ''; search.blur(); }
    if (mobile.matches && menu.open) { menu.open = false; menu.querySelector('summary').focus(); }
  }
});

const expand = document.getElementById('expandCommands');
function updateExpand() {
  const allOpen = commands.every(command => command.open);
  expand.textContent = allOpen ? 'Collapse all' : 'Expand all';
}
commands.forEach(command => command.addEventListener('toggle', updateExpand));
expand.addEventListener('click', () => {
  const open = !commands.every(command => command.open);
  commands.forEach(command => { command.open = open; });
});

// One shared tab pattern, with native buttons and arrow-key navigation.
const tablist = document.querySelector('.install-tabs');
const tabs = [...tablist.querySelectorAll('button')];
tablist.setAttribute('role', 'tablist');
function selectTab(selected) {
  tabs.forEach(tab => {
    const active = selected === tab;
    tab.setAttribute('aria-selected', String(active));
    tab.tabIndex = active ? 0 : -1;
    document.getElementById(tab.getAttribute('aria-controls')).hidden = !active;
  });
}
tabs.forEach((tab, position) => {
  tab.setAttribute('role', 'tab');
  const panel = document.getElementById(tab.getAttribute('aria-controls'));
  panel.setAttribute('role', 'tabpanel');
  panel.setAttribute('aria-labelledby', tab.id);
  panel.tabIndex = 0;
  tab.addEventListener('click', () => selectTab(tab));
  tab.addEventListener('keydown', event => {
    let next;
    if (event.key === 'ArrowRight') next = (position + 1) % tabs.length;
    if (event.key === 'ArrowLeft') next = (position + tabs.length - 1) % tabs.length;
    if (event.key === 'Home') next = 0;
    if (event.key === 'End') next = tabs.length - 1;
    if (next !== undefined) { event.preventDefault(); selectTab(tabs[next]); tabs[next].focus(); }
  });
});
selectTab(tabs[0]);

const outline = document.querySelector('.outline-links');
let currentSection;
function setActive(id) {
  if (currentSection === id) return;
  currentSection = id;
  navLinks.forEach(link => {
    const active = link.hash === '#' + id;
    link.classList.toggle('active', active);
    if (active) link.setAttribute('aria-current', 'location');
    else link.removeAttribute('aria-current');
  });
  const section = document.getElementById(id);
  const headings = [...section.querySelectorAll('h2, h3')];
  outline.replaceChildren();
  (headings.length ? headings : [section.querySelector('h1')]).forEach(heading => {
    const link = document.createElement('a');
    link.href = '#' + (heading.id || id);
    link.textContent = heading.textContent;
    outline.append(link);
  });
}
function revealHash() {
  let id;
  try { id = decodeURIComponent(location.hash.slice(1)); } catch { return; }
  const target = document.getElementById(id);
  if (!target) return;
  const command = target.closest('.command');
  if (command) command.open = true;
  const section = target.closest('.searchable');
  if (section) setActive(section.id);
}
// Open details before the browser follows a command result's native anchor.
document.addEventListener('click', event => {
  const link = event.target.closest('a[href^="#"]');
  if (!link) return;
  const target = document.getElementById(link.hash.slice(1));
  if (!target) return;
  const command = target.closest('.command');
  if (command) command.open = true;
  const section = target.closest('.searchable');
  if (section) setActive(section.id);
  if (mobile.matches) menu.open = false;
  if (link.closest('.search-results')) {
    closeSearch();
    target.setAttribute('tabindex', '-1');
    target.focus({ preventScroll: true });
  }
});
window.addEventListener('hashchange', revealHash);
setActive('overview');
revealHash();

// Observe a reading band. No geometry scan on every scroll or sidebar scrollIntoView.
let observer;
function observeSections() {
  observer?.disconnect();
  const offset = mobile.matches ? 180 : 100;
  observer = new IntersectionObserver(entries => {
    entries.forEach(entry => { if (entry.isIntersecting) setActive(entry.target.id); });
  }, { rootMargin: `-${offset}px 0px -55% 0px`, threshold: 0 });
  sections.forEach(section => observer.observe(section));
}
observeSections();
mobile.addEventListener('change', observeSections);
