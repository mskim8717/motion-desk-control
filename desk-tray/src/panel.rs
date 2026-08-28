// 통합 팝오버 패널 HTML (웹뷰용) — 현재 높이 + ▲▼ 홀드 이동 + 프리셋 + 사용 기록 차트.
// 버튼은 window.ipc.postMessage(...)로 Rust에 전달되고,
// Rust는 setTitle()/setConn()/setPresets()를 evaluate_script로 호출해 상태를 갱신한다.
pub struct PanelState<'a> {
    /// 큰 글씨로 표시할 텍스트 (예: "78cm", "연결 중...")
    pub big: &'a str,
    pub connected: bool,
    pub sit_set: bool,
    pub stand_set: bool,
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
    format!(
        r##"<!doctype html><html><head><meta charset="utf-8"><style>
html,body{{margin:0;height:100%;background:transparent;color:#e5e5e7;
font:13px -apple-system,'Apple SD Gothic Neo',sans-serif;overflow:hidden;
-webkit-user-select:none;user-select:none}}
#wrap{{position:fixed;inset:0;background:rgba(28,28,30,.97);border-radius:14px;
border:1px solid rgba(255,255,255,.12);overflow:hidden;display:flex;flex-direction:column}}
#toprow{{display:flex;align-items:center;justify-content:center;gap:14px;margin-top:10px;height:44px}}
#big{{font-size:32px;font-weight:700}}
#big.small{{font-size:14px;font-weight:600;color:#98989d}}
#arrows{{display:flex;flex-direction:column;gap:4px}}
#arrows button{{width:26px;height:20px;padding:0;border:0;border-radius:6px;background:#3a3a3c;
color:#e5e5e7;font-size:11px;line-height:20px;cursor:default}}
#status{{text-align:center;color:#98989d;font-size:10px;height:13px}}
#btns{{display:flex;gap:6px;padding:8px 12px 0}}
.btnw{{flex:1.4;display:flex}}
.btnw button:first-child{{flex:1;border-radius:8px 0 0 8px}}
.btnw .star{{width:26px;border-radius:0 8px 8px 0;border-left:1px solid rgba(0,0,0,.35);
color:#ffd60a;font-size:13px}}
#bstop{{flex:1}}
button{{padding:7px 2px;border:0;border-radius:8px;background:#3a3a3c;color:#e5e5e7;
font:12px -apple-system,'Apple SD Gothic Neo',sans-serif;cursor:default;white-space:nowrap}}
button:active:not(:disabled){{background:#0a84ff}}
button:disabled{{opacity:.35}}
.label{{text-align:center;color:#98989d;font-size:10px;margin:8px 0 2px;
display:flex;align-items:center;gap:8px;padding:0 14px}}
.label:before,.label:after{{content:'';flex:1;height:1px;background:#3a3a3c}}
canvas{{display:block}}
#summary{{text-align:center;color:#98989d;font-size:11px;padding:2px 0 8px}}
</style></head><body><div id="wrap">
<div id="toprow">
<div id="big"></div>
<div id="arrows">
<button id="bup">&#9650;</button>
<button id="bdn">&#9660;</button>
</div>
</div>
<div id="status"></div>
<div id="btns">
<div class="btnw"><button id="bsit" onclick="cmd('sit')">앉기</button><button
 id="ssit" class="star" onclick="cmd('save_sit')">☆</button></div>
<div class="btnw"><button id="bstand" onclick="cmd('stand')">서기</button><button
 id="sstand" class="star" onclick="cmd('save_stand')">☆</button></div>
<button id="bstop" onclick="cmd('stop')">정지</button>
</div>
<div class="label">사용 기록 (24시간)</div>
<canvas id="c"></canvas>
<div id="summary"></div>
</div>
<script>
const DATA=[{data}],TH={threshold};
let CONN={connected},SIT={sit_set},STAND={stand_set};
const cv=document.getElementById('c'),ctx=cv.getContext('2d');
const $=id=>document.getElementById(id);
function cmd(c){{if(CONN)window.ipc.postMessage(c)}}
// ▲▼: 누르는 동안 이동, 떼거나 벗어나면 정지
for(const [id,m] of [['bup','up'],['bdn','down']]){{
  const el=$(id);
  el.addEventListener('mousedown',()=>cmd(m));
  el.addEventListener('mouseup',()=>cmd('stop'));
  el.addEventListener('mouseleave',()=>{{if(el.matches(':active'))cmd('stop')}});
}}
// 높이 숫자("78cm")만 크게, 상태 문구("연결 중..." 등)는 작게.
// 상태 문구일 때는 아래 status 줄과 중복되므로 status를 숨긴다.
function setTitle(t){{const el=$('big'),h=/^[0-9]+cm$/.test(t);el.textContent=t;
el.classList.toggle('small',!h);$('status').style.visibility=h?'visible':'hidden'}}
function setPresets(sit,stand){{SIT=sit;STAND=stand;
$('ssit').textContent=sit?'★':'☆';$('sstand').textContent=stand?'★':'☆';apply()}}
function setConn(c,label){{CONN=c;$('status').textContent=label;apply()}}
function apply(){{
$('bsit').disabled=!CONN||!SIT;$('bstand').disabled=!CONN||!STAND;
$('bstop').disabled=!CONN;$('bup').disabled=!CONN;$('bdn').disabled=!CONN;
$('ssit').disabled=!CONN;$('sstand').disabled=!CONN}}
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
  const W=innerWidth-28,H=120,dpr=devicePixelRatio||1;
  cv.width=W*dpr;cv.height=H*dpr;cv.style.width=W+'px';cv.style.height=H+'px';
  cv.style.marginLeft='14px';
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
setPresets(SIT,STAND);
setConn(CONN,CONN?'연결됨':'연결 안 됨');
</script></body></html>"##,
        data = data,
        threshold = s.threshold_cm,
        connected = s.connected,
        sit_set = s.sit_set,
        stand_set = s.stand_set,
        big = s.big,
    )
}
