// 사용 기록 차트 HTML 생성 (웹뷰용, 외부 의존성 없는 인라인 canvas)
/// `threshold_cm` 이상을 "서기"로 집계한다.
pub fn html(samples: &[(i64, f32)], threshold_cm: f32) -> String {
    let data = samples
        .iter()
        .map(|(t, c)| format!("[{},{:.1}]", t, c))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r##"<!doctype html><html><head><meta charset="utf-8"><style>
html,body{{margin:0;height:100%;background:#1c1c1e;color:#e5e5e7;
font:13px -apple-system,'Apple SD Gothic Neo',sans-serif;overflow:hidden;
-webkit-user-select:none;user-select:none}}
#head{{display:flex;justify-content:space-between;align-items:baseline;padding:14px 16px 6px}}
#title{{font-weight:600;font-size:15px}} #summary{{color:#98989d}}
canvas{{display:block}}
</style></head><body>
<div id="head"><span id="title">지난 24시간</span><span id="summary"></span></div>
<canvas id="c"></canvas>
<script>
const DATA=[{data}],TH={threshold_cm},NOW=Date.now()/1000,FROM=NOW-86400;
const cv=document.getElementById('c'),ctx=cv.getContext('2d');
function fmt(s){{const h=Math.floor(s/3600),m=Math.round(s%3600/60);return (h?h+'시간 ':'')+m+'분'}}
(function(){{
  if(!DATA.length){{document.getElementById('summary').textContent='아직 기록이 없습니다';return}}
  let stand=0;
  for(let i=0;i<DATA.length;i++){{
    const end=i+1<DATA.length?DATA[i+1][0]:NOW;
    if(DATA[i][1]>=TH)stand+=end-DATA[i][0];
  }}
  document.getElementById('summary').textContent=
    '서기 '+fmt(stand)+' ('+Math.round(stand/864)+'%) · 현재 '+Math.round(DATA[DATA.length-1][1])+'cm';
}})();
function draw(){{
  const W=innerWidth,H=innerHeight-44,dpr=devicePixelRatio||1;
  cv.width=W*dpr;cv.height=H*dpr;cv.style.width=W+'px';cv.style.height=H+'px';
  ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,W,H);
  const L=34,R=10,T=8,B=22,pw=W-L-R,ph=H-T-B;
  const Y0=60,Y1=130;
  const x=t=>L+(t-FROM)/86400*pw, y=c=>T+ph-(c-Y0)/(Y1-Y0)*ph;
  ctx.strokeStyle='#3a3a3c';ctx.fillStyle='#98989d';ctx.lineWidth=1;ctx.font='10px -apple-system';
  for(let c=70;c<130;c+=20){{
    ctx.beginPath();ctx.moveTo(L,y(c));ctx.lineTo(W-R,y(c));ctx.stroke();
    ctx.fillText(c,6,y(c)+3);
  }}
  for(let k=0;k<=4;k++){{
    const t=FROM+k*21600,d=new Date(t*1000);
    ctx.textAlign=k===0?'left':(k===4?'right':'center');
    ctx.fillText(String(d.getHours()).padStart(2,'0')+':00',x(t),H-6);
  }}
  ctx.textAlign='left';
  if(!DATA.length)return;
  // 서기 기준선
  ctx.strokeStyle='rgba(48,209,88,.45)';ctx.setLineDash([4,4]);
  ctx.beginPath();ctx.moveTo(L,y(TH));ctx.lineTo(W-R,y(TH));ctx.stroke();ctx.setLineDash([]);
  // 계단형 영역 + 선
  ctx.beginPath();ctx.moveTo(x(DATA[0][0]),y(DATA[0][1]));
  for(let i=1;i<DATA.length;i++){{
    ctx.lineTo(x(DATA[i][0]),y(DATA[i-1][1]));ctx.lineTo(x(DATA[i][0]),y(DATA[i][1]));
  }}
  ctx.lineTo(x(NOW),y(DATA[DATA.length-1][1]));
  ctx.strokeStyle='#0a84ff';ctx.lineWidth=2;ctx.stroke();
  ctx.lineTo(x(NOW),y(Y0));ctx.lineTo(x(DATA[0][0]),y(Y0));ctx.closePath();
  ctx.fillStyle='rgba(10,132,255,.28)';ctx.fill();
}}
addEventListener('resize',draw);draw();
</script></body></html>"##
    )
}
