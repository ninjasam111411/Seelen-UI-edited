<script lang="ts">
  // The weather popup opened from the toolbar's weather item: today at a glance, the next
  // hours, and the week with a bar for each day's low-to-high range.
  import { onMount } from "svelte";
  import { Settings, Widget } from "@seelen-ui/lib";
  import Icon from "libs/ui/svelte/components/Icon/Icon.svelte";
  import { describeWeather, weather } from "libs/ui/svelte/utils/weather.svelte.ts";
  import { locale, t } from "./i18n/index.ts";

  weather.init();
  // a popup must say it is ready, or Seelen keeps holding its clicks and never shows it
  onMount(() => {
    Widget.getCurrent().ready();
  });
  Settings.getAsync().then((s) => locale.set(s.language));
  Settings.onChange((s) => locale.set(s.language));

  const data = $derived(weather.data);
  const today = $derived(data?.days[0]);
  const now = $derived(data ? describeWeather(data.now.code, data.now.isDay) : null);

  /** the week's coldest low and warmest high, so every day's bar uses the same scale */
  const range = $derived.by(() => {
    if (!data) return { min: 0, max: 1 };
    const min = Math.min(...data.days.map((d) => d.low));
    const max = Math.max(...data.days.map((d) => d.high));
    return { min, max: Math.max(max, min + 1) };
  });

  function dayName(date: string, index: number): string {
    if (index === 0) return $t("today");
    return new Date(`${date}T12:00:00`).toLocaleDateString([], { weekday: "short" });
  }

  function hourName(time: string, index: number): string {
    if (index === 0) return $t("now");
    return new Date(time).toLocaleTimeString([], { hour: "numeric" });
  }

  function clock(time: string): string {
    return new Date(time).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  }

  function barStyle(low: number, high: number): string {
    const span = range.max - range.min;
    const left = ((low - range.min) / span) * 100;
    const width = Math.max(((high - low) / span) * 100, 6);
    return `left:${left}%;width:${width}%`;
  }
</script>

<div class="slu-std-popover weather-popup">
  {#if data && today && now}
    <header class="wp-now">
      <div class="wp-now-main">
        <Icon iconName={now.icon as any} class="wp-now-icon" />
        <span class="wp-now-temp">{Math.round(data.now.temperature)}°</span>
      </div>
      <div class="wp-now-info">
        <span class="wp-place">{data.place}</span>
        <span class="wp-condition">{now.label}</span>
        <span class="wp-muted">{$t("high_low", { 0: String(Math.round(today.high)), 1: String(Math.round(today.low)) })}</span>
        <span class="wp-muted">{$t("feels_like", { 0: String(Math.round(data.now.feelsLike)) })}</span>
      </div>
    </header>

    <div class="wp-chips">
      <span class="wp-chip">{$t("wind", { 0: String(Math.round(data.now.wind)) })}</span>
      <span class="wp-chip">{$t("humidity", { 0: String(Math.round(data.now.humidity)) })}</span>
      <span class="wp-chip">{$t("sunrise")} {clock(today.sunrise)}</span>
      <span class="wp-chip">{$t("sunset")} {clock(today.sunset)}</span>
    </div>

    <section class="wp-section">
      <h2 class="wp-section-title">{$t("next_hours")}</h2>
      <div class="wp-hours">
        {#each data.hours.slice(0, 12) as hour, i (hour.time)}
          {@const look = describeWeather(hour.code, hour.isDay)}
          <div class="wp-hour">
            <span class="wp-muted">{hourName(hour.time, i)}</span>
            <Icon iconName={look.icon as any} class="wp-hour-icon" />
            <span class="wp-hour-temp">{Math.round(hour.temperature)}°</span>
            {#if hour.rainChance >= 20}
              <span class="wp-rain">{hour.rainChance}%</span>
            {/if}
          </div>
        {/each}
      </div>
    </section>

    <section class="wp-section">
      <h2 class="wp-section-title">{$t("this_week")}</h2>
      <div class="wp-days">
        {#each data.days as day, i (day.date)}
          {@const look = describeWeather(day.code, true)}
          <div class="wp-day" title={look.label}>
            <span class="wp-day-name">{dayName(day.date, i)}</span>
            <Icon iconName={look.icon as any} class="wp-day-icon" />
            <span class="wp-rain wp-day-rain">{day.rainChance >= 20 ? `${day.rainChance}%` : ""}</span>
            <span class="wp-day-low">{Math.round(day.low)}°</span>
            <span class="wp-day-track">
              <span class="wp-day-bar" style={barStyle(day.low, day.high)}></span>
            </span>
            <span class="wp-day-high">{Math.round(day.high)}°</span>
          </div>
        {/each}
      </div>
    </section>
  {:else if weather.failed}
    <div class="wp-message">
      <span>{$t("failed")}</span>
      <button data-skin="default" onclick={() => weather.refresh()}>{$t("retry")}</button>
    </div>
  {:else}
    <div class="wp-message">{$t("loading")}</div>
  {/if}
</div>
