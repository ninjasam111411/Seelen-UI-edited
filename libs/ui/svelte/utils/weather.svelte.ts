// Weather for the toolbar item and the weather popup, from Open-Meteo (free, no account).
// Each webview fetches it on first use and refreshes it every 15 minutes.

export interface WeatherNow {
  temperature: number;
  feelsLike: number;
  code: number;
  isDay: boolean;
  wind: number;
  humidity: number;
}

export interface WeatherHour {
  /** local time, ISO */
  time: string;
  temperature: number;
  code: number;
  rainChance: number;
  isDay: boolean;
}

export interface WeatherDay {
  /** local date, YYYY-MM-DD */
  date: string;
  code: number;
  high: number;
  low: number;
  rainChance: number;
  sunrise: string;
  sunset: string;
}

export interface WeatherData {
  place: string;
  now: WeatherNow;
  hours: WeatherHour[];
  days: WeatherDay[];
  updatedAt: number;
}

/** Charlotte, NC */
export const WEATHER_PLACE = { name: "Charlotte", latitude: 35.2271, longitude: -80.8431 };
const REFRESH_MS = 15 * 60 * 1000;
const RETRY_MS = 60 * 1000;

let data = $state.raw<WeatherData | null>(null);
let failed = $state(false);
let started = false;

function url(): string {
  const params = new URLSearchParams({
    latitude: String(WEATHER_PLACE.latitude),
    longitude: String(WEATHER_PLACE.longitude),
    current: "temperature_2m,apparent_temperature,weather_code,is_day,wind_speed_10m,relative_humidity_2m",
    hourly: "temperature_2m,weather_code,precipitation_probability,is_day",
    daily: "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset",
    temperature_unit: "fahrenheit",
    wind_speed_unit: "mph",
    timezone: "America/New_York",
    forecast_days: "7",
  });
  return `https://api.open-meteo.com/v1/forecast?${params}`;
}

async function load() {
  try {
    const response = await fetch(url());
    if (!response.ok) throw new Error(`weather: ${response.status}`);
    const json = await response.json();
    const c = json.current;
    const h = json.hourly;
    const d = json.daily;
    // hourly times are local ("2026-11-25T17:00"): start from the current hour
    const localNow = new Date(Date.now() - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 13);
    const start = Math.max(0, (h.time as string[]).findIndex((t) => t.slice(0, 13) >= localNow));
    data = {
      place: WEATHER_PLACE.name,
      now: {
        temperature: c.temperature_2m,
        feelsLike: c.apparent_temperature,
        code: c.weather_code,
        isDay: c.is_day === 1,
        wind: c.wind_speed_10m,
        humidity: c.relative_humidity_2m,
      },
      hours: (h.time as string[]).slice(start, start + 24).map((time, i) => ({
        time,
        temperature: h.temperature_2m[start + i],
        code: h.weather_code[start + i],
        rainChance: h.precipitation_probability[start + i] ?? 0,
        isDay: h.is_day[start + i] === 1,
      })),
      days: (d.time as string[]).map((date, i) => ({
        date,
        code: d.weather_code[i],
        high: d.temperature_2m_max[i],
        low: d.temperature_2m_min[i],
        rainChance: d.precipitation_probability_max[i] ?? 0,
        sunrise: d.sunrise[i],
        sunset: d.sunset[i],
      })),
      updatedAt: Date.now(),
    };
    failed = false;
    setTimeout(load, REFRESH_MS);
  } catch (e) {
    console.error("Could not load the weather", e);
    failed = true;
    setTimeout(load, RETRY_MS);
  }
}

/** What a WMO weather code means, with an icon name (react-icons, Lucide set) and an emoji. */
export function describeWeather(code: number, isDay = true): { label: string; icon: string; emoji: string } {
  if (code === 0) {
    return isDay
      ? { label: "Sunny", icon: "LuSun", emoji: "☀️" }
      : { label: "Clear", icon: "LuMoon", emoji: "🌙" };
  }
  if (code === 1 || code === 2) {
    return isDay
      ? { label: "Partly cloudy", icon: "LuCloudSun", emoji: "⛅" }
      : { label: "Partly cloudy", icon: "LuCloudMoon", emoji: "☁️" };
  }
  if (code === 3) return { label: "Cloudy", icon: "LuCloud", emoji: "☁️" };
  if (code === 45 || code === 48) return { label: "Fog", icon: "LuCloudFog", emoji: "🌫️" };
  if (code >= 51 && code <= 57) return { label: "Drizzle", icon: "LuCloudDrizzle", emoji: "🌦️" };
  if ((code >= 61 && code <= 67) || (code >= 80 && code <= 82)) {
    return { label: code >= 65 && code !== 80 ? "Heavy rain" : "Rain", icon: "LuCloudRain", emoji: "🌧️" };
  }
  if ((code >= 71 && code <= 77) || code === 85 || code === 86) {
    return { label: "Snow", icon: "LuCloudSnow", emoji: "🌨️" };
  }
  if (code >= 95) return { label: "Thunderstorms", icon: "LuCloudLightning", emoji: "⛈️" };
  return { label: "Cloudy", icon: "LuCloud", emoji: "☁️" };
}

class WeatherState {
  get data() {
    return data;
  }
  get failed() {
    return failed;
  }
  get ready() {
    return data !== null || failed;
  }
  /** starts loading on first use */
  init() {
    if (started) return;
    started = true;
    load();
  }
  refresh() {
    load();
  }
}

export const weather = new WeatherState();
