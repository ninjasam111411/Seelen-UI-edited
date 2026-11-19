// Draws a disc icon for the dock.
const ctx = canvas.getContext("2d");
if (!ctx) return;

const w = canvas.width;
const h = canvas.height;
const cx = w / 2;
const cy = h / 2;
const r = w * 0.44;

ctx.clearRect(0, 0, w, h);

const accent = isDarkMode ? systemTokens.accentLightColor : systemTokens.accentDarkColor;

// disc body with a rainbow sheen
const sheen = ctx.createConicGradient(0, cx, cy);
sheen.addColorStop(0, "#d9dde3");
sheen.addColorStop(0.15, "#9fd3ff");
sheen.addColorStop(0.3, "#e9d6ff");
sheen.addColorStop(0.5, "#d9dde3");
sheen.addColorStop(0.65, "#ffe1b8");
sheen.addColorStop(0.8, "#b8ffe4");
sheen.addColorStop(1, "#d9dde3");
ctx.fillStyle = sheen;
ctx.beginPath();
ctx.arc(cx, cy, r, 0, Math.PI * 2);
ctx.fill();

// label ring in the user accent
ctx.fillStyle = accent;
ctx.beginPath();
ctx.arc(cx, cy, r * 0.42, 0, Math.PI * 2);
ctx.fill();

// center hole
ctx.globalCompositeOperation = "destination-out";
ctx.beginPath();
ctx.arc(cx, cy, r * 0.13, 0, Math.PI * 2);
ctx.fill();
ctx.globalCompositeOperation = "source-over";

ctx.strokeStyle = themeTokens.foregroundColor;
ctx.globalAlpha = 0.25;
ctx.lineWidth = w * 0.015;
ctx.beginPath();
ctx.arc(cx, cy, r, 0, Math.PI * 2);
ctx.stroke();
ctx.globalAlpha = 1;
