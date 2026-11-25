// Console-style interface sounds, synthesized with the Web Audio API (no sound files): a soft
// tick when moving between games, a chime when the console turns on, a whoosh for a disc going
// in and a confirm tone for starting a game. Kept quiet, like a console's menu sounds.

const VOLUME = 0.18;

let context: AudioContext | null = null;

function audio(): AudioContext | null {
  try {
    context ??= new AudioContext();
    if (context.state === "suspended") context.resume().catch(() => {});
    return context;
  } catch {
    return null;
  }
}

interface Tone {
  frequency: number;
  /** frequency at the end, for a glide */
  to?: number;
  start?: number;
  duration: number;
  type?: OscillatorType;
  volume?: number;
}

function play(tones: Tone[]) {
  const ctx = audio();
  if (!ctx) return;
  const now = ctx.currentTime;
  for (const tone of tones) {
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    const start = now + (tone.start ?? 0);
    const end = start + tone.duration;
    osc.type = tone.type ?? "sine";
    osc.frequency.setValueAtTime(tone.frequency, start);
    if (tone.to) osc.frequency.exponentialRampToValueAtTime(tone.to, end);
    // quick fade in and a smooth fade out, so nothing clicks
    const peak = VOLUME * (tone.volume ?? 1);
    gain.gain.setValueAtTime(0.0001, start);
    gain.gain.exponentialRampToValueAtTime(peak, start + 0.012);
    gain.gain.exponentialRampToValueAtTime(0.0001, end);
    osc.connect(gain).connect(ctx.destination);
    osc.start(start);
    osc.stop(end + 0.05);
  }
}

function noise(duration: number, from: number, to: number, volume = 1) {
  const ctx = audio();
  if (!ctx) return;
  const now = ctx.currentTime;
  const buffer = ctx.createBuffer(1, Math.ceil(ctx.sampleRate * duration), ctx.sampleRate);
  const data = buffer.getChannelData(0);
  for (let i = 0; i < data.length; i++) data[i] = Math.random() * 2 - 1;
  const source = ctx.createBufferSource();
  source.buffer = buffer;
  const filter = ctx.createBiquadFilter();
  filter.type = "bandpass";
  filter.Q.value = 1.2;
  filter.frequency.setValueAtTime(from, now);
  filter.frequency.exponentialRampToValueAtTime(to, now + duration);
  const gain = ctx.createGain();
  gain.gain.setValueAtTime(0.0001, now);
  gain.gain.exponentialRampToValueAtTime(VOLUME * 0.8 * volume, now + duration * 0.3);
  gain.gain.exponentialRampToValueAtTime(0.0001, now + duration);
  source.connect(filter).connect(gain).connect(ctx.destination);
  source.start(now);
}

export const sounds = {
  /** moving between games */
  move: () => play([{ frequency: 1400, duration: 0.05, type: "triangle", volume: 0.5 }]),
  /** a button or a choice */
  select: () =>
    play([
      { frequency: 880, duration: 0.08, type: "triangle", volume: 0.6 },
      { frequency: 1320, start: 0.05, duration: 0.1, type: "sine", volume: 0.5 },
    ]),
  back: () => play([{ frequency: 700, to: 420, duration: 0.12, type: "triangle", volume: 0.5 }]),
  /** the console turning on */
  boot: () =>
    play([
      { frequency: 220, to: 440, duration: 0.5, type: "sine", volume: 0.7 },
      { frequency: 523, start: 0.25, duration: 0.9, type: "sine", volume: 0.5 },
      { frequency: 784, start: 0.4, duration: 1.0, type: "sine", volume: 0.4 },
      { frequency: 1047, start: 0.55, duration: 1.1, type: "sine", volume: 0.3 },
    ]),
  /** a disc sliding into the console */
  insert: () => noise(0.45, 2400, 300),
  /** a game starting */
  start: () =>
    play([
      { frequency: 523, duration: 0.18, type: "sine", volume: 0.6 },
      { frequency: 659, start: 0.12, duration: 0.2, type: "sine", volume: 0.6 },
      { frequency: 1047, start: 0.24, duration: 0.5, type: "sine", volume: 0.5 },
    ]),
};
