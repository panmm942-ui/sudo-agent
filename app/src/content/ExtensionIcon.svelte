<script lang="ts">
 let {name='sparkles'}:{name?:string}=$props();
 const paths:Record<string,string>={
  sparkles:'m12 3 2.2 6.8L21 12l-6.8 2.2L12 21l-2.2-6.8L3 12l6.8-2.2Z',
  mail:'M3 5h18v14H3Zm0 1 9 7 9-7',
  calendar:'M4 5h16v16H4Zm0 5h16M8 3v4m8-4v4',
  check:'m5 12 4 4L19 6',
  chart:'M4 20V4m0 16h17M8 16v-5m5 5V6m5 10v-8',
  globe:'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0ZM3 12h18M12 3c5 5 5 13 0 18-5-5-5-13 0-18',
  code:'m8 6-6 6 6 6m8-12 6 6-6 6m-3-15-2 18',
  note:'M5 3h14v18H5Zm4 5h6m-6 4h6m-6 4h4',
  timer:'M20 13a8 8 0 1 1-16 0 8 8 0 0 1 16 0ZM9 2h6m-3 7v4l3 2',
  cloud:'M6 18a4 4 0 0 1-1-8 6 6 0 0 1 11-2 5 5 0 0 1 2 10Z',
  music:'M9 18V5l11-2v13M9 7l11-2M9 18c0 3-6 4-6 1s6-4 6-1m11-2c0 3-6 4-6 1s6-4 6-1',
  bolt:'m13 2-9 12h7l-1 8 10-13h-7Z',
 };
</script>
<svg viewBox="0 0 24 24" width="19" height="19" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={paths[name]??paths.sparkles}/></svg>
