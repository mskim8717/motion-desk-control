// 통합 팝오버 패널 HTML (웹뷰용) — 현재 높이 + 조작 버튼 + 사용 기록 차트.
// 버튼은 window.ipc.postMessage('sit'|'stand'|'stop')로 Rust에 전달되고,
// Rust는 setTitle()/setConn()을 evaluate_script로 호출해 상태를 갱신한다.
pub struct PanelState<'a> {
    /// 큰 글씨로 표시할 텍스트 (예: "78cm", "연결 중...")
    pub big: &'a str,
    pub connected: bool,
    pub sit_label: Option<String>,
    pub stand_label: Option<String>,
    /// (unix초, cm) 샘플
    pub samples: &'a [(i64, f32)],
    /// 서기 판정 기준 높이
    pub threshold_cm: f32,
}

pub fn html(s: &PanelState) -> String {
    let data = s
        .samples
        .iter()
        .map(|(t, c)| format!("[{},{:.1}]", t, c))
        .collect::<Vec<_>>()
        .join(",");
    let (sit_label, sit_ok) = match &s.sit_label {
        Some(l) => (l.clone(), true),
        None => ("앉기".into(), false),
    };
    let (stand_label, stand_ok) = match &s.stand_label {
        Some(l) => (l.clone(), true),
        None => ("서기".into(), false),
    };
    format!(
        r##"<!doctype html><html><head><meta charset="utf-8"><style>
html,body{{margin:0;height:100%;background:transparent;color:#e5e5e7;
font:13px -apple-system,'Apple SD Gothic Neo',sans-serif;overflow:hidden;
-webkit-user-select:none;user-select:none}}
#wrap{{position:fixed;inset:0;background:rgba(28,28,30,.97);border-radius:14px;
border:1px solid rgba(255,255,255,.12);overflow:hidden;display:flex;flex-direction:column}}
#big{{display:flex;align-items:center;justify-content:center;height:46px;margin-top:16px;
font-size:38px;font-weight:700}}
#big.small{{font-size:15px;font-weight:600;color:#98989d}}
#status{{text-align:center;color:#98989d;margin-top:2px;font-size:11px}}
#btns{{display:flex;gap:6px;padding:12px 14px 2px}}
button{{flex:1;padding:8px 2px;border:0;border-radius:8px;background:#3a3a3c;color:#e5e5e7;
font:12px -apple-system,'Apple SD Gothic Neo',sans-serif;cursor:default;white-space:nowrap}}
button:active{{background:#0a84ff}}
button:disabled{{opacity:.35}}
.label{{text-align:center;color:#98989d;font-size:11px;margin:12px 0 4px;
display:flex;align-items:center;gap:10px;padding:0 16px}}
.label:before,.label:after{{content:'';flex:1;height:1px;background:#3a3a3c}}
canvas{{display:block}}
#summary{{text-align:center;color:#98989d;padding:6px 0 12px}}
</style></head><body><div id="wrap">
<div id="big"></div><div id="status"></div>
<div id="btns">
<button id="bsit" onclick="cmd('sit')">{sit_label}</button>
<button id="bstand" onclick="cmd('stand')">{stand_label}</button>
<button id="bstop" onclick="cmd('stop')">정지</button>
</div>
<div class="label">사용 기록 (24시간)</div>
<canvas id="c"></canvas>
<div id="summary"></div>
</div>
<script>
const DATA=[{data}],TH={threshold},SIT_OK={sit_ok},STAND_OK={stand_ok};
let CONN={connected};
const cv=document.getElementById('c'),ctx=cv.getContext('2d');
const $=id=>document.getElementById(id);
function cmd(c){{if(CONN)window.ipc.postMessage(c)}}
// 높이 숫자("78cm")만 크게, 상태 문구("연결 중..." 등)는 작게
function setTitle(t){{const el=$('big');el.textContent=t;
el.classList.toggle('small',!/^[0-9]+cm$/.test(t))}}
function setConn(c,label){{CONN=c;$('status').textContent=label;
$('bsit').disabled=!c||!SIT_OK;$('bstand').disabled=!c||!STAND_OK;$('bstop').disabled=!c}}
function fmt(s){{const h=Math.floor(s/3600),m=Math.round(s%3600/60);return (h?h+'시간 ':'')+m+'분'}}
(function(){{
  if(!DATA.length){{$('summary').textContent='아직 기록이 없습니다';return}}
  const NOW=Date.now()/1000;let stand=0;
  for(let i=0;i<DATA.length;i++){{
    const end=i+1<DATA.length?DATA[i+1][0]:NOW;
    if(DATA[i][1]>=TH)stand+=end-DATA[i][0];
  }}
  $('summary').textContent='서기 '+fmt(stand)+' ('+Math.round(stand/864)+'%)';
}})();
function draw(){{
  const NOW=Date.now()/1000,FROM=NOW-86400;
  const W=innerWidth-32,H=150,dpr=devicePixelRatio||1;
  cv.width=W*dpr;cv.height=H*dpr;cv.style.width=W+'px';cv.style.height=H+'px';
  cv.style.marginLeft='16px';
  ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,W,H);
  const L=26,R=2,T=6,B=18,pw=W-L-R,ph=H-T-B;
  const Y0=60,Y1=130;
  const x=t=>L+(t-FROM)/86400*pw, y=c=>T+ph-(c-Y0)/(Y1-Y0)*ph;
  ctx.strokeStyle='#3a3a3c';ctx.fillStyle='#98989d';ctx.lineWidth=1;ctx.font='9px -apple-system';
  for(let c=70;c<130;c+=20){{
    ctx.beginPath();ctx.moveTo(L,y(c));ctx.lineTo(W-R,y(c));ctx.stroke();
    ctx.fillText(c,2,y(c)+3);
  }}
  for(let k=0;k<=4;k++){{
    const t=FROM+k*21600,d=new Date(t*1000);
    ctx.textAlign=k===0?'left':(k===4?'right':'center');
    ctx.fillText(String(d.getHours()).padStart(2,'0')+':00',x(t),H-4);
  }}
  ctx.textAlign='left';
  if(!DATA.length)return;
  ctx.strokeStyle='rgba(48,209,88,.45)';ctx.setLineDash([4,4]);
  ctx.beginPath();ctx.moveTo(L,y(TH));ctx.lineTo(W-R,y(TH));ctx.stroke();ctx.setLineDash([]);
  ctx.beginPath();ctx.moveTo(x(Math.max(DATA[0][0],FROM)),y(DATA[0][1]));
  for(let i=1;i<DATA.length;i++){{
    ctx.lineTo(x(DATA[i][0]),y(DATA[i-1][1]));ctx.lineTo(x(DATA[i][0]),y(DATA[i][1]));
  }}
  ctx.lineTo(x(NOW),y(DATA[DATA.length-1][1]));
  ctx.strokeStyle='#0a84ff';ctx.lineWidth=2;ctx.stroke();
  ctx.lineTo(x(NOW),y(Y0));ctx.lineTo(x(Math.max(DATA[0][0],FROM)),y(Y0));ctx.closePath();
  ctx.fillStyle='rgba(10,132,255,.28)';ctx.fill();
}}
addEventListener('resize',draw);draw();
setTitle('{big}');
setConn(CONN,CONN?'연결됨':'연결 안 됨');
</script></body></html>"##,
        big = s.big,
        sit_label = sit_label,
        stand_label = stand_label,
        data = data,
        threshold = s.threshold_cm,
        sit_ok = sit_ok,
        stand_ok = stand_ok,
        connected = s.connected,
    )
}
