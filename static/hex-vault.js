/**
 * Hexagonal Vault Background
 * High-performance 2D canvas honeycomb grid with a soft cursor spotlight.
 */

export function initHexVault(canvasId = 'heroVault', containerSelector = '.landing-hero') {
  const canvas = document.getElementById(canvasId);
  if (!canvas) return;

  const container = canvas.closest(containerSelector) || canvas.parentElement;
  const ctx = canvas.getContext('2d', { alpha: true });
  if (!ctx) return;

  const prefersReducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  // Grid & lighting parameters
  const HEX_RADIUS = 28; // Radius of each hexagon
  const LIGHT_RADIUS = 320; // Radius of cursor spotlight in px
  const AMBIENT_LINE = 'rgba(255, 255, 255, 0.035)'; // Idle hexagon grid edge
  const ACCENT_RGB = '247, 108, 124'; // LightMem accent-2 (#f76c7c)
  const ACCENT_PRIMARY_RGB = '228, 75, 93'; // LightMem accent (#e44b5d)

  // Pointy-topped hexagon dimensions
  const HEX_WIDTH = Math.sqrt(3) * HEX_RADIUS;
  const HEX_VERT_SPACING = HEX_RADIUS * 1.5;

  let width = 0;
  let height = 0;
  let dpr = 1;
  let hexagons = [];
  let animId = null;
  let isVisible = !('IntersectionObserver' in window);
  let intersectionObserver = null;
  let cachedRect = null;

  function updateRect() {
    cachedRect = container.getBoundingClientRect();
  }

  // Cursor state with smooth lerp interpolation
  const mouse = {
    x: -9999,
    y: -9999,
    targetX: -9999,
    targetY: -9999,
    active: false,
    hovered: false,
  };

  // Idle ambient sweep when mouse is not active
  let idleTime = 0;

  function resize() {
    updateRect();
    width = cachedRect.width;
    height = cachedRect.height;
    dpr = Math.min(window.devicePixelRatio || 1, 2);

    canvas.width = Math.floor(width * dpr);
    canvas.height = Math.floor(height * dpr);
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;

    computeHexGrid();

    // Default resting light position (over right side / terminal)
    if (!mouse.hovered) {
      mouse.targetX = width * 0.72;
      mouse.targetY = height * 0.38;
      if (mouse.x < -1000) {
        mouse.x = mouse.targetX;
        mouse.y = mouse.targetY;
      }
    }

    draw();
  }

  function computeHexGrid() {
    hexagons = [];
    const cols = Math.ceil(width / HEX_WIDTH) + 2;
    const rows = Math.ceil(height / HEX_VERT_SPACING) + 2;

    for (let r = -1; r <= rows; r++) {
      const y = r * HEX_VERT_SPACING;
      const xOffset = (r % 2 === 0) ? 0 : HEX_WIDTH / 2;

      for (let c = -1; c <= cols; c++) {
        const x = c * HEX_WIDTH + xOffset;
        hexagons.push({ x, y });
      }
    }
  }

  function drawHexagonPath(context, cx, cy, r) {
    context.beginPath();
    for (let i = 0; i < 6; i++) {
      const angle = (Math.PI / 180) * (60 * i - 30);
      const px = cx + r * Math.cos(angle);
      const py = cy + r * Math.sin(angle);
      if (i === 0) context.moveTo(px, py);
      else context.lineTo(px, py);
    }
    context.closePath();
  }

  function draw() {
    ctx.save();
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, width, height);

    // 1. Pass: Batch ambient grid lines (single stroke call for peak performance)
    ctx.lineWidth = 1;
    ctx.strokeStyle = AMBIENT_LINE;
    ctx.beginPath();
    for (let i = 0; i < hexagons.length; i++) {
      const hex = hexagons[i];
      for (let j = 0; j < 6; j++) {
        const angle = (Math.PI / 180) * (60 * j - 30);
        const px = hex.x + HEX_RADIUS * Math.cos(angle);
        const py = hex.y + HEX_RADIUS * Math.sin(angle);
        if (j === 0) ctx.moveTo(px, py);
        else ctx.lineTo(px, py);
      }
    }
    ctx.stroke();

    // 2. Pass: Lit hexagons in proximity to the cursor light
    const curX = mouse.x;
    const curY = mouse.y;
    const lightRadiusSq = LIGHT_RADIUS * LIGHT_RADIUS;

    for (let i = 0; i < hexagons.length; i++) {
      const hex = hexagons[i];
      const dx = hex.x - curX;
      const dy = hex.y - curY;
      const distSq = dx * dx + dy * dy;

      if (distSq < lightRadiusSq) {
        const dist = Math.sqrt(distSq);
        const intensity = Math.max(0, 1 - dist / LIGHT_RADIUS);
        const ease = intensity * intensity; // quadratic falloff

        drawHexagonPath(ctx, hex.x, hex.y, HEX_RADIUS - 0.5);

        // Soft internal glow inside lit hexagons
        ctx.fillStyle = `rgba(${ACCENT_PRIMARY_RGB}, ${ease * 0.09})`;
        ctx.fill();

        // Gleaming hexagon edge
        ctx.strokeStyle = `rgba(${ACCENT_RGB}, ${0.1 + ease * 0.65})`;
        ctx.lineWidth = 1 + ease * 0.8;
        ctx.stroke();
      }
    }

    // 3. Pass: Atmospheric soft radial light diffusion
    if (mouse.x > -500 && mouse.y > -500) {
      const glowGrad = ctx.createRadialGradient(curX, curY, 0, curX, curY, LIGHT_RADIUS * 0.85);
      glowGrad.addColorStop(0, `rgba(${ACCENT_RGB}, 0.14)`);
      glowGrad.addColorStop(0.35, `rgba(${ACCENT_PRIMARY_RGB}, 0.06)`);
      glowGrad.addColorStop(0.7, `rgba(${ACCENT_PRIMARY_RGB}, 0.015)`);
      glowGrad.addColorStop(1, 'rgba(0, 0, 0, 0)');

      ctx.fillStyle = glowGrad;
      ctx.beginPath();
      ctx.arc(curX, curY, LIGHT_RADIUS * 0.85, 0, Math.PI * 2);
      ctx.fill();
    }

    ctx.restore();
  }

  function startLoop() {
    if (prefersReducedMotion || !isVisible || animId !== null) return;
    animId = requestAnimationFrame(loop);
  }

  function stopLoop() {
    if (animId !== null) {
      cancelAnimationFrame(animId);
      animId = null;
    }
  }

  function loop() {
    if (prefersReducedMotion || !isVisible) {
      animId = null;
      return;
    }

    // Smooth lerp tracking
    const dx = mouse.targetX - mouse.x;
    const dy = mouse.targetY - mouse.y;
    mouse.x += dx * 0.08;
    mouse.y += dy * 0.08;

    // Subtle gentle breathing drift when mouse is not manually moving
    if (!mouse.hovered) {
      idleTime += 0.018;
      mouse.targetX = (width * 0.72) + Math.sin(idleTime) * 36;
      mouse.targetY = (height * 0.38) + Math.cos(idleTime * 0.8) * 24;
    }

    draw();
    animId = requestAnimationFrame(loop);
  }

  // Pointer event handlers
  function onPointerMove(e) {
    if (!cachedRect) updateRect();
    const x = e.clientX - cachedRect.left;
    const y = e.clientY - cachedRect.top;

    if (x >= -50 && x <= cachedRect.width + 50 && y >= -50 && y <= cachedRect.height + 50) {
      mouse.hovered = true;
      mouse.targetX = x;
      mouse.targetY = y;
      if (prefersReducedMotion) {
        mouse.x = x;
        mouse.y = y;
        draw();
      }
    }
  }

  function onPointerLeave() {
    mouse.hovered = false;
  }

  // Event Listeners
  container.addEventListener('pointerenter', updateRect, { passive: true });
  container.addEventListener('pointermove', onPointerMove, { passive: true });
  container.addEventListener('pointerleave', onPointerLeave, { passive: true });
  window.addEventListener('scroll', updateRect, { passive: true });

  const resizeObserver = new ResizeObserver(() => {
    resize();
  });
  resizeObserver.observe(container);

  if ('IntersectionObserver' in window) {
    intersectionObserver = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        if (entry.target === canvas) {
          isVisible = entry.isIntersecting;
          if (isVisible) {
            startLoop();
          } else {
            stopLoop();
          }
        }
      }
    }, { threshold: 0 });
    intersectionObserver.observe(canvas);
  }

  // Initialize
  resize();
  startLoop();

  return () => {
    stopLoop();
    container.removeEventListener('pointerenter', updateRect);
    container.removeEventListener('pointermove', onPointerMove);
    container.removeEventListener('pointerleave', onPointerLeave);
    window.removeEventListener('scroll', updateRect);
    resizeObserver.disconnect();
    if (intersectionObserver) intersectionObserver.disconnect();
  };
}

// Auto-boot if container and canvas are in document
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', () => initHexVault());
} else {
  initHexVault();
}
