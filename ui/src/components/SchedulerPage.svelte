<script lang="ts">
  import { onMount } from "svelte";
  import { tauriApi, type ScheduleRule } from "../lib/api";
  import type { DownloadStore } from "../lib/store.svelte";
  let { store, onclose }: { store: DownloadStore; onclose: () => void } = $props();
  const weekdays = [{n:1,label:"Mon"},{n:2,label:"Tue"},{n:3,label:"Wed"},{n:4,label:"Thu"},{n:5,label:"Fri"},{n:6,label:"Sat"},{n:7,label:"Sun"}];
  let rules = $state<ScheduleRule[]>([]);
  let editing = $state<string | null>(null);
  let name = $state(""); let days = $state<number[]>([1,2,3,4,5]);
  let start = $state("09:00"); let stop = $state("17:00"); let timezone = $state("Asia/Dhaka"); let limitKib = $state(0); let enabled = $state(true);
  onMount(() => {
    void (async () => {
      try { rules = await tauriApi.getScheduleRules(); } catch (e) { store.say(String(e), true); }
    })();
  });
  function reset() { editing = null; name = ""; days = [1,2,3,4,5]; start = "09:00"; stop = "17:00"; timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC"; limitKib = 0; enabled = true; }
  function edit(rule: ScheduleRule) { editing = rule.id; name = rule.name; days = [...rule.days]; start = rule.start_time; stop = rule.stop_time; timezone = rule.timezone; limitKib = rule.speed_limit ? Math.round(rule.speed_limit / 1024) : 0; enabled = rule.enabled; }
  function toggleDay(day: number) { days = days.includes(day) ? days.filter(d => d !== day) : [...days, day].sort((a,b) => a-b); }
  async function save() {
    const rule: ScheduleRule = { id: editing || crypto.randomUUID(), name: name.trim(), days, start_time: start, stop_time: stop, timezone: timezone.trim(), speed_limit: limitKib > 0 ? Math.round(limitKib * 1024) : null, enabled };
    try { const next = editing ? rules.map(r => r.id === editing ? rule : r) : [...rules, rule]; await tauriApi.saveScheduleRules(next); rules = next; reset(); }
    catch (e) { store.say(String(e), true); }
  }
  async function remove(id: string) { const next = rules.filter(r => r.id !== id); try { await tauriApi.saveScheduleRules(next); rules = next; if (editing === id) reset(); } catch (e) { store.say(String(e), true); } }
  async function toggle(rule: ScheduleRule) { const next = rules.map(r => r.id === rule.id ? {...r, enabled: !r.enabled} : r); try { await tauriApi.saveScheduleRules(next); rules = next; } catch (e) { store.say(String(e), true); } }
</script>
<section class="scheduler-page">
  <header><div><h2>Scheduler</h2><p>Rules use their selected time zone. Start times resume queued downloads; stop times pause active downloads.</p></div><button type="button" onclick={onclose}>Back to downloads</button></header>
  <div class="scheduler-layout">
    <form class="scheduler-form" onsubmit={(e) => { e.preventDefault(); void save(); }}>
      <h3>{editing ? "Edit rule" : "Add rule"}</h3>
      <label>Rule name <input required maxlength="80" bind:value={name} /></label>
      <fieldset><legend>Days</legend><div class="day-options">{#each weekdays as day}<label><input type="checkbox" checked={days.includes(day.n)} onchange={() => toggleDay(day.n)} />{day.label}</label>{/each}</div></fieldset>
      <div class="time-grid"><label>Start <input type="time" required bind:value={start} /></label><label>Stop <input type="time" required bind:value={stop} /></label></div>
      <label>Time zone (IANA) <input required bind:value={timezone} placeholder="Asia/Dhaka" /></label>
      <label>Speed limit (KiB/s, 0 = no limit) <input type="number" min="0" max="1048576" bind:value={limitKib} /></label>
      <label class="inline-check"><input type="checkbox" bind:checked={enabled} /> Enabled</label>
      <div class="dialog-actions"><button type="submit" disabled={!name.trim() || days.length === 0}>Save rule</button>{#if editing}<button type="button" onclick={reset}>Cancel edit</button>{/if}</div>
    </form>
    <div class="schedule-list"><h3>Rules</h3>
      {#if rules.length === 0}<p>No schedule rules yet.</p>{/if}
      {#each rules as rule (rule.id)}
        <article class="schedule-rule"><div><strong>{rule.name}</strong><p>{rule.days.map(d => weekdays[d-1]?.label).join(", ")} · {rule.start_time}–{rule.stop_time} · {rule.timezone}</p><small>{rule.speed_limit ? `${Math.round(rule.speed_limit / 1024)} KiB/s` : "No speed limit"}</small></div><div class="schedule-actions"><label class="inline-check"><input type="checkbox" checked={rule.enabled} onchange={() => void toggle(rule)} /> Enabled</label><button type="button" onclick={() => edit(rule)}>Edit</button><button type="button" class="danger" onclick={() => void remove(rule.id)}>Delete</button></div></article>
      {/each}
    </div>
  </div>
</section>
