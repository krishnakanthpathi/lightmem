use crate::models::{GraphEdge, GraphNode, GraphSnapshot};
use anyhow::Result;
use colored::Colorize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub fn generate_html(snapshot: &GraphSnapshot) -> String {
    let json_data = serde_json::to_string(snapshot)
        .unwrap_or_else(|_| "{\"nodes\":[],\"edges\":[]}".to_string());

    format!(r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>LightMem · Interactive Knowledge Graph</title>
  <style>
    * {{
      box-sizing: border-box;
      margin: 0;
      padding: 0;
      user-select: none;
    }}
    body {{
      background-color: #0b0c10;
      color: #e2e8f0;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
      overflow: hidden;
      width: 100vw;
      height: 100vh;
    }}
    #graph-container {{
      position: absolute;
      top: 0;
      left: 0;
      width: 100%;
      height: 100%;
      background: radial-gradient(circle at center, #13141f 0%, #0b0c10 100%);
    }}
    canvas {{
      display: block;
      width: 100%;
      height: 100%;
    }}
    /* Top Bar HUD */
    .top-bar {{
      position: absolute;
      top: 16px;
      left: 16px;
      right: 16px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      pointer-events: none;
      z-index: 10;
    }}
    .hud-card {{
      pointer-events: auto;
      background: rgba(18, 20, 29, 0.85);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      border: 1px solid rgba(255, 255, 255, 0.08);
      border-radius: 12px;
      padding: 10px 16px;
      box-shadow: 0 8px 32px rgba(0, 0, 0, 0.45);
      display: flex;
      align-items: center;
      gap: 12px;
    }}
    .logo {{
      font-weight: 700;
      font-size: 14px;
      letter-spacing: 1px;
      display: flex;
      align-items: center;
      gap: 8px;
      color: #f8fafc;
    }}
    .logo-icon {{
      color: #6366f1;
      font-size: 16px;
    }}
    .stats-badge {{
      font-size: 12px;
      color: #94a3b8;
      background: rgba(255, 255, 255, 0.06);
      padding: 4px 8px;
      border-radius: 6px;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    }}
    .search-box {{
      position: relative;
      display: flex;
      align-items: center;
    }}
    .search-input {{
      pointer-events: auto;
      background: rgba(15, 17, 26, 0.9);
      border: 1px solid rgba(255, 255, 255, 0.12);
      border-radius: 8px;
      padding: 8px 12px 8px 32px;
      color: #f1f5f9;
      font-size: 13px;
      outline: none;
      width: 240px;
      transition: all 0.2s ease;
    }}
    .search-input:focus {{
      border-color: #6366f1;
      width: 320px;
      box-shadow: 0 0 0 2px rgba(99, 102, 241, 0.25);
    }}
    .search-icon {{
      position: absolute;
      left: 10px;
      color: #64748b;
      font-size: 13px;
    }}
    .btn {{
      pointer-events: auto;
      background: rgba(255, 255, 255, 0.07);
      border: 1px solid rgba(255, 255, 255, 0.1);
      color: #cbd5e1;
      border-radius: 8px;
      padding: 8px 14px;
      font-size: 12px;
      font-weight: 500;
      cursor: pointer;
      display: flex;
      align-items: center;
      gap: 6px;
      transition: all 0.15s ease;
    }}
    .btn:hover {{
      background: rgba(255, 255, 255, 0.12);
      color: #ffffff;
      border-color: rgba(255, 255, 255, 0.2);
    }}
    /* Categories filter bar */
    .filter-bar {{
      position: absolute;
      bottom: 20px;
      left: 50%;
      transform: translateX(-50%);
      pointer-events: auto;
      background: rgba(18, 20, 29, 0.85);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      border: 1px solid rgba(255, 255, 255, 0.08);
      border-radius: 30px;
      padding: 6px 12px;
      display: flex;
      align-items: center;
      gap: 6px;
      box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
      z-index: 10;
      max-width: 90vw;
      overflow-x: auto;
    }}
    .category-pill {{
      display: flex;
      align-items: center;
      gap: 6px;
      font-size: 11px;
      padding: 4px 10px;
      border-radius: 20px;
      cursor: pointer;
      background: transparent;
      border: 1px solid transparent;
      color: #94a3b8;
      transition: all 0.15s ease;
      white-space: nowrap;
    }}
    .category-pill:hover {{
      color: #e2e8f0;
      background: rgba(255, 255, 255, 0.05);
    }}
    .category-pill.active {{
      background: rgba(255, 255, 255, 0.1);
      border-color: rgba(255, 255, 255, 0.15);
      color: #ffffff;
    }}
    .category-dot {{
      width: 7px;
      height: 7px;
      border-radius: 50%;
    }}
    /* Floating Tooltip */
    .node-tooltip {{
      position: absolute;
      pointer-events: none;
      background: rgba(15, 17, 26, 0.95);
      backdrop-filter: blur(12px);
      border: 1px solid rgba(255, 255, 255, 0.12);
      border-radius: 8px;
      padding: 8px 12px;
      font-size: 12px;
      box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
      max-width: 280px;
      z-index: 20;
      display: none;
      transform: translate(-50%, -100%) translateY(-12px);
      transition: opacity 0.15s ease;
    }}
    .tooltip-header {{
      display: flex;
      align-items: center;
      gap: 6px;
      font-weight: 600;
      margin-bottom: 4px;
      color: #f8fafc;
    }}
    .tooltip-badge {{
      font-size: 10px;
      padding: 1px 6px;
      border-radius: 4px;
      text-transform: uppercase;
      font-weight: 700;
      letter-spacing: 0.5px;
    }}
    .tooltip-snippet {{
      color: #94a3b8;
      font-size: 11px;
      line-height: 1.4;
      display: -webkit-box;
      -webkit-line-clamp: 3;
      -webkit-box-orient: vertical;
      overflow: hidden;
    }}
    /* Right Sliding Detail Drawer */
    .drawer {{
      position: absolute;
      top: 16px;
      right: 16px;
      bottom: 16px;
      width: 400px;
      max-width: calc(100vw - 32px);
      background: rgba(18, 20, 29, 0.95);
      backdrop-filter: blur(24px);
      -webkit-backdrop-filter: blur(24px);
      border: 1px solid rgba(255, 255, 255, 0.1);
      border-radius: 16px;
      box-shadow: -10px 10px 40px rgba(0, 0, 0, 0.6);
      display: flex;
      flex-direction: column;
      z-index: 30;
      transform: translateX(calc(100% + 24px));
      transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1);
      user-select: text;
    }}
    .drawer.open {{
      transform: translateX(0);
    }}
    .drawer-header {{
      padding: 16px 20px;
      border-bottom: 1px solid rgba(255, 255, 255, 0.08);
      display: flex;
      align-items: center;
      justify-content: space-between;
    }}
    .drawer-title {{
      font-size: 15px;
      font-weight: 600;
      color: #f8fafc;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }}
    .drawer-close {{
      background: none;
      border: none;
      color: #94a3b8;
      font-size: 18px;
      cursor: pointer;
      border-radius: 6px;
      width: 28px;
      height: 28px;
      display: flex;
      align-items: center;
      justify-content: center;
      transition: all 0.15s ease;
    }}
    .drawer-close:hover {{
      color: #ffffff;
      background: rgba(255, 255, 255, 0.08);
    }}
    .drawer-body {{
      padding: 20px;
      overflow-y: auto;
      flex: 1;
      display: flex;
      flex-direction: column;
      gap: 16px;
    }}
    .drawer-badge-row {{
      display: flex;
      align-items: center;
      gap: 8px;
    }}
    .drawer-category-badge {{
      font-size: 11px;
      font-weight: 700;
      text-transform: uppercase;
      padding: 3px 8px;
      border-radius: 5px;
      letter-spacing: 0.5px;
    }}
    .drawer-confidence {{
      font-size: 12px;
      color: #94a3b8;
      font-family: ui-monospace, SFMono-Regular, monospace;
    }}
    .confidence-meter {{
      flex: 1;
      height: 4px;
      background: rgba(255, 255, 255, 0.08);
      border-radius: 2px;
      overflow: hidden;
    }}
    .confidence-fill {{
      height: 100%;
      background: #10b981;
      border-radius: 2px;
    }}
    .drawer-content-box {{
      background: rgba(11, 12, 16, 0.6);
      border: 1px solid rgba(255, 255, 255, 0.06);
      border-radius: 8px;
      padding: 14px;
      font-size: 13px;
      line-height: 1.6;
      color: #e2e8f0;
      white-space: pre-wrap;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      max-height: 280px;
      overflow-y: auto;
    }}
    .drawer-tags {{
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
    }}
    .tag-chip {{
      font-size: 11px;
      background: rgba(99, 102, 241, 0.15);
      color: #a5b4fc;
      border: 1px solid rgba(99, 102, 241, 0.25);
      border-radius: 6px;
      padding: 2px 8px;
    }}
    .drawer-section-title {{
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.8px;
      color: #64748b;
      font-weight: 700;
      margin-top: 4px;
    }}
    .neighbors-list {{
      display: flex;
      flex-direction: column;
      gap: 6px;
    }}
    .neighbor-item {{
      background: rgba(255, 255, 255, 0.03);
      border: 1px solid rgba(255, 255, 255, 0.06);
      border-radius: 8px;
      padding: 8px 12px;
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
      transition: all 0.15s ease;
    }}
    .neighbor-item:hover {{
      background: rgba(255, 255, 255, 0.08);
      border-color: rgba(255, 255, 255, 0.15);
    }}
    .neighbor-title {{
      font-size: 12px;
      color: #f1f5f9;
      font-weight: 500;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }}
    .neighbor-rel {{
      font-size: 10px;
      background: rgba(255, 255, 255, 0.06);
      color: #94a3b8;
      padding: 2px 6px;
      border-radius: 4px;
      font-family: ui-monospace, monospace;
    }}
  </style>
</head>
<body>
  <div id="graph-container">
    <canvas id="graph-canvas"></canvas>
  </div>

  <!-- Top Bar HUD -->
  <div class="top-bar">
    <div class="hud-card">
      <div class="logo">
        <span class="logo-icon">❖</span>
        <span>LIGHTMEM GRAPH</span>
      </div>
      <div class="stats-badge" id="stats-badge">0 nodes · 0 links</div>
    </div>

    <div class="hud-card">
      <div class="search-box">
        <span class="search-icon">⌕</span>
        <input type="text" class="search-input" id="search-input" placeholder="Search title, content, tag...">
      </div>
      <button class="btn" id="btn-reset">⟲ Reset Zoom</button>
    </div>
  </div>

  <!-- Categories Filter Bar -->
  <div class="filter-bar" id="filter-bar">
    <div class="category-pill active" data-cat="all">
      <div class="category-dot" style="background:#cbd5e1;"></div>
      <span>All Categories</span>
    </div>
  </div>

  <!-- Floating Hover Tooltip -->
  <div class="node-tooltip" id="node-tooltip">
    <div class="tooltip-header">
      <span class="tooltip-badge" id="tt-badge">FACT</span>
      <span id="tt-title">Memory Title</span>
    </div>
    <div class="tooltip-snippet" id="tt-snippet">Memory snippet description goes here...</div>
  </div>

  <!-- Detail Drawer -->
  <div class="drawer" id="drawer">
    <div class="drawer-header">
      <div class="drawer-title" id="drawer-title">Memory Details</div>
      <button class="drawer-close" id="drawer-close">✕</button>
    </div>
    <div class="drawer-body">
      <div class="drawer-badge-row">
        <span class="drawer-category-badge" id="drawer-badge">FACT</span>
        <div class="confidence-meter">
          <div class="confidence-fill" id="drawer-conf-fill" style="width: 80%;"></div>
        </div>
        <span class="drawer-confidence" id="drawer-conf-text">90%</span>
      </div>

      <div class="drawer-tags" id="drawer-tags"></div>

      <div class="drawer-section-title">Content</div>
      <div class="drawer-content-box" id="drawer-content"></div>

      <div class="drawer-section-title">Connected Memories (<span id="drawer-conn-count">0</span>)</div>
      <div class="neighbors-list" id="drawer-neighbors"></div>
    </div>
  </div>

  <script>
    const graphData = {json_data};

    // Category Color Palette
    const CATEGORY_COLORS = {{
      'fact': '#10b981',        // Emerald
      'decision': '#8b5cf6',    // Violet
      'instruction': '#06b6d4', // Cyan
      'preference': '#ec4899',  // Pink
      'learning': '#3b82f6',    // Blue
      'goal': '#f59e0b',        // Amber
      'commitment': '#f97316',  // Orange
      'artifact': '#6366f1',    // Indigo
      'event': '#14b8a6',       // Teal
      'relationship': '#d946ef',// Fuchsia
      'observation': '#84cc16', // Lime
      'error': '#ef4444',       // Rose
      'context': '#a855f7',     // Purple
      'password': '#eab308',    // Gold
      'default': '#94a3b8'
    }};

    function getCategoryColor(cat) {{
      return CATEGORY_COLORS[cat ? cat.toLowerCase() : 'default'] || CATEGORY_COLORS['default'];
    }}

    // Canvas setup
    const canvas = document.getElementById('graph-canvas');
    const ctx = canvas.getContext('2d');
    let width = window.innerWidth;
    let height = window.innerHeight;

    function resize() {{
      const dpr = window.devicePixelRatio || 1;
      width = window.innerWidth;
      height = window.innerHeight;
      canvas.width = width * dpr;
      canvas.height = height * dpr;
      ctx.scale(dpr, dpr);
    }}
    window.addEventListener('resize', resize);
    resize();

    // Setup Category Filter Pills
    const filterBar = document.getElementById('filter-bar');
    const activeCategories = new Set(graphData.nodes.map(n => n.category.toLowerCase()));
    let selectedCategory = 'all';

    activeCategories.forEach(cat => {{
      const pill = document.createElement('div');
      pill.className = 'category-pill';
      pill.dataset.cat = cat;
      pill.innerHTML = `
        <div class="category-dot" style="background:${{getCategoryColor(cat)}};"></div>
        <span style="text-transform:capitalize;">${{cat}}</span>
      `;
      pill.addEventListener('click', () => {{
        document.querySelectorAll('.category-pill').forEach(p => p.classList.remove('active'));
        pill.classList.add('active');
        selectedCategory = cat;
      }});
      filterBar.appendChild(pill);
    }});

    document.querySelector('.category-pill[data-cat="all"]').addEventListener('click', (e) => {{
      document.querySelectorAll('.category-pill').forEach(p => p.classList.remove('active'));
      e.currentTarget.classList.add('active');
      selectedCategory = 'all';
    }});

    // Graph Data Structures
    const nodeMap = new Map();
    const nodes = graphData.nodes.map(n => {{
      const baseRadius = 6 + Math.min(n.degree * 2, 16);
      const node = {{
        ...n,
        x: (Math.random() - 0.5) * (width * 0.7) + width / 2,
        y: (Math.random() - 0.5) * (height * 0.7) + height / 2,
        vx: 0,
        vy: 0,
        radius: baseRadius,
        color: getCategoryColor(n.category)
      }};
      nodeMap.set(n.id, node);
      return node;
    }});

    const edges = graphData.edges.filter(e => nodeMap.has(e.source) && nodeMap.has(e.target)).map(e => ({{
      ...e,
      sourceNode: nodeMap.get(e.source),
      targetNode: nodeMap.get(e.target)
    }}));

    document.getElementById('stats-badge').innerText = `${{nodes.length}} nodes · ${{edges.length}} links`;

    // Camera Transform (Pan & Zoom)
    let zoom = 1.0;
    let panX = 0;
    let panY = 0;
    let isPanning = false;
    let startPanX = 0;
    let startPanY = 0;

    // Node Dragging & Selection
    let draggedNode = null;
    let hoveredNode = null;
    let selectedNode = null;
    let searchQuery = '';

    // Physics Simulation Engine
    let simulationAlpha = 1.0;
    const repulsionStrength = 900;
    const springLength = 85;
    const springStrength = 0.04;
    const centerGravity = 0.015;
    const damping = 0.82;

    function stepPhysics() {{
      if (simulationAlpha < 0.002) return;

      const centerX = width / 2;
      const centerY = height / 2;

      // 1. Repulsion between all node pairs
      for (let i = 0; i < nodes.length; i++) {{
        const a = nodes[i];
        for (let j = i + 1; j < nodes.length; j++) {{
          const b = nodes[j];
          let dx = b.x - a.x;
          let dy = b.y - a.y;
          let distSq = dx * dx + dy * dy || 1;
          let dist = Math.sqrt(distSq);
          if (dist < 400) {{
            let force = (repulsionStrength / (distSq + 20)) * simulationAlpha;
            let fx = (dx / dist) * force;
            let fy = (dy / dist) * force;
            a.vx -= fx;
            a.vy -= fy;
            b.vx += fx;
            b.vy += fy;
          }}
        }}

        // Center gravity
        a.vx += (centerX - a.x) * centerGravity * simulationAlpha;
        a.vy += (centerY - a.y) * centerGravity * simulationAlpha;
      }}

      // 2. Attraction along edges
      for (let i = 0; i < edges.length; i++) {{
        const edge = edges[i];
        const a = edge.sourceNode;
        const b = edge.targetNode;
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let dist = Math.sqrt(dx * dx + dy * dy) || 1;
        let force = (dist - springLength) * springStrength * simulationAlpha;
        let fx = (dx / dist) * force;
        let fy = (dy / dist) * force;
        a.vx += fx;
        a.vy += fy;
        b.vx -= fx;
        b.vy -= fy;
      }}

      // 3. Position update with damping
      for (let i = 0; i < nodes.length; i++) {{
        const n = nodes[i];
        if (n !== draggedNode) {{
          n.vx *= damping;
          n.vy *= damping;
          n.x += n.vx;
          n.y += n.vy;
        }} else {{
          n.vx = 0;
          n.vy = 0;
        }}
      }}

      simulationAlpha *= 0.985;
    }}

    // Warm up initial simulation
    for (let i = 0; i < 60; i++) stepPhysics();

    // Render loop
    function render() {{
      stepPhysics();

      ctx.clearRect(0, 0, width, height);

      ctx.save();
      ctx.translate(panX, panY);
      ctx.scale(zoom, zoom);

      // Connected nodes set for active hover/selection
      const activeNeighborIds = new Set();
      if (hoveredNode || selectedNode) {{
        const focus = hoveredNode || selectedNode;
        activeNeighborIds.add(focus.id);
        edges.forEach(e => {{
          if (e.source === focus.id) activeNeighborIds.add(e.target);
          if (e.target === focus.id) activeNeighborIds.add(e.source);
        }});
      }}

      // 1. Draw Edges
      for (let i = 0; i < edges.length; i++) {{
        const edge = edges[i];
        const a = edge.sourceNode;
        const b = edge.targetNode;

        const isFiltered = (selectedCategory !== 'all') && 
          (a.category.toLowerCase() !== selectedCategory && b.category.toLowerCase() !== selectedCategory);

        if (isFiltered) continue;

        let isEdgeActive = false;
        if (hoveredNode || selectedNode) {{
          const focus = hoveredNode || selectedNode;
          isEdgeActive = (a.id === focus.id || b.id === focus.id);
        }}

        ctx.beginPath();
        ctx.moveTo(a.x, a.y);
        ctx.lineTo(b.x, b.y);

        if (hoveredNode || selectedNode) {{
          if (isEdgeActive) {{
            ctx.strokeStyle = a.color;
            ctx.lineWidth = 2.2 / zoom;
            ctx.globalAlpha = 0.95;
          }} else {{
            ctx.strokeStyle = 'rgba(255, 255, 255, 0.05)';
            ctx.lineWidth = 0.8 / zoom;
            ctx.globalAlpha = 0.15;
          }}
        }} else {{
          ctx.strokeStyle = 'rgba(255, 255, 255, 0.14)';
          ctx.lineWidth = 1.0 / zoom;
          ctx.globalAlpha = 0.6;
        }}
        ctx.stroke();

        // Draw arrow if zoomed in or edge active
        if (zoom > 0.8 || isEdgeActive) {{
          const midX = (a.x + b.x) / 2;
          const midY = (a.y + b.y) / 2;
          const angle = Math.atan2(b.y - a.y, b.x - a.x);
          const arrowLength = (isEdgeActive ? 7 : 5) / zoom;
          ctx.beginPath();
          ctx.moveTo(midX, midY);
          ctx.lineTo(midX - arrowLength * Math.cos(angle - Math.PI / 6), midY - arrowLength * Math.sin(angle - Math.PI / 6));
          ctx.lineTo(midX - arrowLength * Math.cos(angle + Math.PI / 6), midY - arrowLength * Math.sin(angle + Math.PI / 6));
          ctx.fillStyle = isEdgeActive ? a.color : 'rgba(255, 255, 255, 0.25)';
          ctx.fill();
        }}
      }}

      // 2. Draw Nodes
      for (let i = 0; i < nodes.length; i++) {{
        const node = nodes[i];

        // Filter visibility
        const isCatMatch = selectedCategory === 'all' || node.category.toLowerCase() === selectedCategory;
        const isSearchMatch = !searchQuery || 
          node.label.toLowerCase().includes(searchQuery) || 
          node.snippet.toLowerCase().includes(searchQuery) ||
          node.tags.some(t => t.toLowerCase().includes(searchQuery));

        let nodeAlpha = 1.0;
        if (!isCatMatch) {{
          nodeAlpha = 0.1;
        }} else if (searchQuery && !isSearchMatch) {{
          nodeAlpha = 0.15;
        }} else if ((hoveredNode || selectedNode) && !activeNeighborIds.has(node.id)) {{
          nodeAlpha = 0.25;
        }}

        ctx.globalAlpha = nodeAlpha;

        const isHovered = hoveredNode === node;
        const isSelected = selectedNode === node;
        const radius = isHovered || isSelected ? node.radius * 1.3 : node.radius;

        // Outer glow on hover or search match
        if ((isHovered || isSelected || (searchQuery && isSearchMatch)) && isCatMatch) {{
          ctx.beginPath();
          ctx.arc(node.x, node.y, radius + 5, 0, Math.PI * 2);
          ctx.fillStyle = node.color;
          ctx.globalAlpha = 0.3;
          ctx.fill();
          ctx.globalAlpha = nodeAlpha;
        }}

        // Main node circle
        ctx.beginPath();
        ctx.arc(node.x, node.y, radius, 0, Math.PI * 2);
        ctx.fillStyle = node.color;
        ctx.fill();
        ctx.lineWidth = 1.5 / zoom;
        ctx.strokeStyle = '#ffffff';
        ctx.stroke();

        // Node Label (show if zoom is high enough, or if hovered/selected/search matched)
        const showLabel = isHovered || isSelected || (searchQuery && isSearchMatch) || (zoom > 1.2 && isCatMatch);
        if (showLabel && nodeAlpha > 0.4) {{
          ctx.font = `${{Math.max(10, 11 / zoom)}}px -apple-system, sans-serif`;
          ctx.fillStyle = '#f8fafc';
          ctx.textAlign = 'center';
          ctx.fillText(node.label, node.x, node.y + radius + 12 / zoom);
        }}
      }}

      ctx.restore();
      requestAnimationFrame(render);
    }}
    requestAnimationFrame(render);

    // Coordinate Transforms
    function screenToWorld(sx, sy) {{
      return {{
        x: (sx - panX) / zoom,
        y: (sy - panY) / zoom
      }};
    }}

    function findNodeAt(sx, sy) {{
      const p = screenToWorld(sx, sy);
      for (let i = nodes.length - 1; i >= 0; i--) {{
        const n = nodes[i];
        const dx = p.x - n.x;
        const dy = p.y - n.y;
        if (dx * dx + dy * dy <= (n.radius + 6) * (n.radius + 6)) {{
          return n;
        }}
      }}
      return null;
    }}

    // Tooltip Handling
    const tooltip = document.getElementById('node-tooltip');
    const ttBadge = document.getElementById('tt-badge');
    const ttTitle = document.getElementById('tt-title');
    const ttSnippet = document.getElementById('tt-snippet');

    function showTooltip(node, sx, sy) {{
      ttBadge.innerText = node.category.toUpperCase();
      ttBadge.style.background = node.color;
      ttBadge.style.color = '#000000';
      ttTitle.innerText = node.label;
      ttSnippet.innerText = node.snippet;
      tooltip.style.left = `${{sx}}px`;
      tooltip.style.top = `${{sy}}px`;
      tooltip.style.display = 'block';
    }}

    function hideTooltip() {{
      tooltip.style.display = 'none';
    }}

    // Drawer Handling
    const drawer = document.getElementById('drawer');
    const drawerTitle = document.getElementById('drawer-title');
    const drawerBadge = document.getElementById('drawer-badge');
    const drawerConfFill = document.getElementById('drawer-conf-fill');
    const drawerConfText = document.getElementById('drawer-conf-text');
    const drawerTags = document.getElementById('drawer-tags');
    const drawerContent = document.getElementById('drawer-content');
    const drawerConnCount = document.getElementById('drawer-conn-count');
    const drawerNeighbors = document.getElementById('drawer-neighbors');

    function openDrawer(node) {{
      selectedNode = node;
      drawerTitle.innerText = node.label;
      drawerBadge.innerText = node.category.toUpperCase();
      drawerBadge.style.background = node.color;
      drawerBadge.style.color = '#000000';

      const confPct = Math.round(node.confidence * 100);
      drawerConfFill.style.width = `${{confPct}}%`;
      drawerConfText.innerText = `${{confPct}}%`;

      drawerTags.innerHTML = '';
      if (node.tags && node.tags.length > 0) {{
        node.tags.forEach(t => {{
          const chip = document.createElement('span');
          chip.className = 'tag-chip';
          chip.innerText = `#${{t}}`;
          drawerTags.appendChild(chip);
        }});
      }}

      drawerContent.innerText = node.snippet;

      // Populate Neighbors
      drawerNeighbors.innerHTML = '';
      const connectedEdges = edges.filter(e => e.source === node.id || e.target === node.id);
      drawerConnCount.innerText = connectedEdges.length;

      connectedEdges.forEach(e => {{
        const isOutgoing = e.source === node.id;
        const neighbor = isOutgoing ? e.targetNode : e.sourceNode;
        const item = document.createElement('div');
        item.className = 'neighbor-item';
        item.innerHTML = `
          <div style="display:flex; align-items:center; gap:8px; overflow:hidden;">
            <div style="width:8px; height:8px; border-radius:50%; background:${{neighbor.color}}; flex-shrink:0;"></div>
            <div class="neighbor-title">${{neighbor.label}}</div>
          </div>
          <div class="neighbor-rel">${{isOutgoing ? '──▶' : '◀──'}} ${{e.relation}}</div>
        `;
        item.addEventListener('click', () => {{
          panToNode(neighbor);
          openDrawer(neighbor);
        }});
        drawerNeighbors.appendChild(item);
      }});

      drawer.classList.add('open');
    }}

    function closeDrawer() {{
      selectedNode = null;
      drawer.classList.remove('open');
    }}
    document.getElementById('drawer-close').addEventListener('click', closeDrawer);

    function panToNode(node) {{
      zoom = 1.3;
      panX = width / 2 - node.x * zoom;
      panY = height / 2 - node.y * zoom;
    }}

    // Mouse & Touch Interaction
    canvas.addEventListener('mousedown', (e) => {{
      const target = findNodeAt(e.clientX, e.clientY);
      if (target) {{
        draggedNode = target;
        simulationAlpha = Math.max(simulationAlpha, 0.4);
      }} else {{
        isPanning = true;
        startPanX = e.clientX - panX;
        startPanY = e.clientY - panY;
      }}
    }});

    window.addEventListener('mousemove', (e) => {{
      if (draggedNode) {{
        const p = screenToWorld(e.clientX, e.clientY);
        draggedNode.x = p.x;
        draggedNode.y = p.y;
        simulationAlpha = Math.max(simulationAlpha, 0.2);
        hideTooltip();
      }} else if (isPanning) {{
        panX = e.clientX - startPanX;
        panY = e.clientY - startPanY;
      }} else {{
        const target = findNodeAt(e.clientX, e.clientY);
        if (target !== hoveredNode) {{
          hoveredNode = target;
          if (hoveredNode) {{
            showTooltip(hoveredNode, e.clientX, e.clientY);
            canvas.style.cursor = 'pointer';
          }} else {{
            hideTooltip();
            canvas.style.cursor = 'default';
          }}
        }} else if (hoveredNode) {{
          tooltip.style.left = `${{e.clientX}}px`;
          tooltip.style.top = `${{e.clientY}}px`;
        }}
      }}
    }});

    window.addEventListener('mouseup', (e) => {{
      if (draggedNode) {{
        draggedNode = null;
      }}
      isPanning = false;
    }});

    canvas.addEventListener('click', (e) => {{
      const target = findNodeAt(e.clientX, e.clientY);
      if (target) {{
        openDrawer(target);
      }}
    }});

    // Zoom on wheel
    canvas.addEventListener('wheel', (e) => {{
      e.preventDefault();
      const zoomFactor = e.deltaY < 0 ? 1.12 : 0.89;
      const mouseX = e.clientX;
      const mouseY = e.clientY;

      const newZoom = Math.max(0.2, Math.min(zoom * zoomFactor, 4.0));
      panX = mouseX - (mouseX - panX) * (newZoom / zoom);
      panY = mouseY - (mouseY - panY) * (newZoom / zoom);
      zoom = newZoom;
    }}, {{ passive: false }});

    // Reset View Button
    document.getElementById('btn-reset').addEventListener('click', () => {{
      zoom = 1.0;
      panX = 0;
      panY = 0;
      simulationAlpha = 0.5;
    }});

    // Live Search
    const searchInput = document.getElementById('search-input');
    searchInput.addEventListener('input', (e) => {{
      searchQuery = e.target.value.trim().toLowerCase();
    }});

    // Esc key closes drawer
    window.addEventListener('keydown', (e) => {{
      if (e.key === 'Escape') {{
        closeDrawer();
      }}
    }});
  </script>
</body>
</html>
"##, json_data = json_data)
}

pub fn render_terminal(snapshot: &GraphSnapshot) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "\n  {} {}\n",
        "❖".bold().bright_magenta(),
        "L I G H T M E M   K N O W L E D G E   G R A P H".bold()
    ));
    out.push_str(&format!(
        "  {}\n",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
    ));
    out.push_str(&format!(
        "  {} {} active memories  ·  {} direct relations\n\n",
        "◈".bright_cyan(),
        snapshot.nodes.len().to_string().bold(),
        snapshot.edges.len().to_string().bold()
    ));

    if snapshot.nodes.is_empty() {
        out.push_str("  No memories or connections found.\n\n");
        return out;
    }

    let mut edges_by_src: HashMap<String, Vec<&GraphEdge>> = HashMap::new();
    for edge in &snapshot.edges {
        edges_by_src
            .entry(edge.source.clone())
            .or_default()
            .push(edge);
    }

    let node_map: HashMap<String, &GraphNode> = snapshot
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n))
        .collect();

    // Sort nodes: hubs with highest degree first
    let mut sorted_nodes = snapshot.nodes.clone();
    sorted_nodes.sort_by(|a, b| b.degree.cmp(&a.degree));

    let mut visited_roots = HashSet::new();

    for node in &sorted_nodes {
        if node.degree == 0 && visited_roots.len() > 10 {
            continue; // Truncate long list of isolated nodes in terminal
        }
        if visited_roots.contains(&node.id) {
            continue;
        }
        visited_roots.insert(node.id.clone());

        let short_id = if node.id.len() >= 8 {
            &node.id[..8]
        } else {
            &node.id
        };
        out.push_str(&format!(
            "  {} [{}] {} ({})\n",
            "◈".bright_blue(),
            node.category.to_lowercase().bright_yellow(),
            node.label.bold(),
            short_id.bright_black()
        ));

        if let Some(out_edges) = edges_by_src.get(&node.id) {
            for (idx, edge) in out_edges.iter().enumerate() {
                let is_last = idx == out_edges.len() - 1;
                let branch = if is_last { "└──" } else { "├──" };
                let target_label = node_map
                    .get(&edge.target)
                    .map(|n| format!("[{}] {}", n.category.to_lowercase(), n.label))
                    .unwrap_or_else(|| edge.target.clone());

                out.push_str(&format!(
                    "  │   {} ──{}──▶ {}\n",
                    branch.bright_black(),
                    edge.relation.bright_cyan(),
                    target_label
                ));
            }
        }
        out.push('\n');
    }

    out
}

pub fn export_and_open_html(
    snapshot: &GraphSnapshot,
    output_path: Option<&Path>,
) -> Result<PathBuf> {
    let html = generate_html(snapshot);
    let target = if let Some(p) = output_path {
        p.to_path_buf()
    } else {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let default_dir = home.join(".lightmem");
        std::fs::create_dir_all(&default_dir)?;
        default_dir.join("graph.html")
    };

    std::fs::write(&target, html)?;

    // Launch in system default browser
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(&target).spawn();

    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", target.to_str().unwrap_or("graph.html")])
        .spawn();

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = std::process::Command::new("xdg-open").arg(&target).spawn();

    Ok(target)
}
