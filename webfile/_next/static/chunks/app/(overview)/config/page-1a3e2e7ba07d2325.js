(self.webpackChunk_N_E=self.webpackChunk_N_E||[]).push([[412],{93:function(e,t,r){Promise.resolve().then(r.bind(r,4161))},4161:function(e,t,r){"use strict";r.r(t),r.d(t,{default:function(){return j}});var n=r(881),s=r(8580),o=r(3335),a=r(7597),i=r(7126);let l=async()=>(await i.h.get("/config")).data,c=async e=>(await i.h.post("/config",e)).data;var d=r(9077),u=r(6749),m=r(1184),f=r(5082),x=r(2491);let h=x.z.object({password:x.z.string(),url:x.z.string(),username:x.z.string()}),p=x.z.object({path:x.z.string(),qbit_config:h.optional()});var v=r(8766);function g(){return(0,n.jsxs)("div",{children:[(0,n.jsx)(v.O,{className:"h-10 w-48 mb-6"}),(0,n.jsxs)("div",{className:"space-y-4",children:[(0,n.jsx)(v.O,{className:"h-8 w-full max-w-sm"}),(0,n.jsx)(v.O,{className:"h-10 w-full max-w-sm"}),(0,n.jsxs)("div",{className:"mt-6",children:[(0,n.jsx)(v.O,{className:"h-8 w-40 mb-2"}),(0,n.jsxs)("div",{className:"space-y-2",children:[(0,n.jsx)(v.O,{className:"h-10 w-full max-w-sm"}),(0,n.jsx)(v.O,{className:"h-10 w-full max-w-sm"}),(0,n.jsx)(v.O,{className:"h-10 w-full max-w-sm"})]})]}),(0,n.jsx)(v.O,{className:"h-10 w-24 mt-4"})]})]})}function j(){var e,t,r;let{data:i,isLoading:x,mutate:h}=(0,d.ZP)("config",l),v=(0,u.cI)({resolver:(0,f.F)(p),defaultValues:{path:(null==i?void 0:i.path)||"",qbit_config:{username:(null==i?void 0:null===(e=i.qbit_config)||void 0===e?void 0:e.username)||"",password:(null==i?void 0:null===(t=i.qbit_config)||void 0===t?void 0:t.password)||"",url:(null==i?void 0:null===(r=i.qbit_config)||void 0===r?void 0:r.url)||""}}}),j=async e=>{await c(e),h(),a.Am.success("配置已更新")};return x?(0,n.jsx)(g,{}):(0,n.jsxs)("div",{children:[(0,n.jsx)("h1",{className:"text-3xl font-bold mb-6",children:"系统配置"}),(0,n.jsx)(m.l0,{...v,children:(0,n.jsxs)("form",{onSubmit:v.handleSubmit(j),className:"space-y-4",children:[(0,n.jsx)(m.Wi,{control:v.control,name:"path",render:e=>{let{field:t}=e;return(0,n.jsxs)(m.xJ,{children:[(0,n.jsx)(m.lX,{children:"下载路径"}),(0,n.jsx)(m.NI,{children:(0,n.jsx)(o.I,{...t})}),(0,n.jsx)(m.zG,{})]})}}),(0,n.jsxs)("div",{children:[(0,n.jsx)("h2",{className:"text-xl font-semibold mb-2",children:"qBittorrent 配置"}),(0,n.jsxs)("div",{className:"space-y-2",children:[(0,n.jsx)(m.Wi,{control:v.control,name:"qbit_config.url",render:e=>{let{field:t}=e;return(0,n.jsxs)(m.xJ,{children:[(0,n.jsx)(m.NI,{children:(0,n.jsx)(o.I,{placeholder:"qBittorrent URL",...t})}),(0,n.jsx)(m.zG,{})]})}}),(0,n.jsx)(m.Wi,{control:v.control,name:"qbit_config.username",render:e=>{let{field:t}=e;return(0,n.jsxs)(m.xJ,{children:[(0,n.jsx)(m.NI,{children:(0,n.jsx)(o.I,{placeholder:"qBittorrent 用户名",...t})}),(0,n.jsx)(m.zG,{})]})}}),(0,n.jsx)(m.Wi,{control:v.control,name:"qbit_config.password",render:e=>{let{field:t}=e;return(0,n.jsxs)(m.xJ,{children:[(0,n.jsx)(m.NI,{children:(0,n.jsx)(o.I,{type:"password",placeholder:"qBittorrent 密码",...t})}),(0,n.jsx)(m.zG,{})]})}})]})]}),(0,n.jsx)(s.z,{type:"submit",children:"更新配置"})]})})]})}},8580:function(e,t,r){"use strict";r.d(t,{z:function(){return c}});var n=r(881),s=r(4149),o=r(4098),a=r(116),i=r(270);let l=(0,a.j)("inline-flex items-center justify-center whitespace-nowrap rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50",{variants:{variant:{default:"bg-primary text-primary-foreground shadow hover:bg-primary/90",destructive:"bg-destructive text-destructive-foreground shadow-sm hover:bg-destructive/90",outline:"border border-input bg-background shadow-sm hover:bg-accent hover:text-accent-foreground",secondary:"bg-secondary text-secondary-foreground shadow-sm hover:bg-secondary/80",ghost:"hover:bg-accent hover:text-accent-foreground",link:"text-primary underline-offset-4 hover:underline"},size:{default:"h-9 px-4 py-2",sm:"h-8 rounded-md px-3 text-xs",lg:"h-10 rounded-md px-8",icon:"h-9 w-9"}},defaultVariants:{variant:"default",size:"default"}}),c=s.forwardRef((e,t)=>{let{className:r,variant:s,size:a,asChild:c=!1,...d}=e,u=c?o.g7:"button";return(0,n.jsx)(u,{className:(0,i.cn)(l({variant:s,size:a,className:r})),ref:t,...d})});c.displayName="Button"},1184:function(e,t,r){"use strict";r.d(t,{l0:function(){return u},NI:function(){return g},pf:function(){return j},Wi:function(){return f},xJ:function(){return p},lX:function(){return v},zG:function(){return b}});var n=r(881),s=r(4149),o=r(4098),a=r(6749),i=r(270),l=r(1212);let c=(0,r(116).j)("text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70"),d=s.forwardRef((e,t)=>{let{className:r,...s}=e;return(0,n.jsx)(l.f,{ref:t,className:(0,i.cn)(c(),r),...s})});d.displayName=l.f.displayName;let u=a.RV,m=s.createContext({}),f=e=>{let{...t}=e;return(0,n.jsx)(m.Provider,{value:{name:t.name},children:(0,n.jsx)(a.Qr,{...t})})},x=()=>{let e=s.useContext(m),t=s.useContext(h),{getFieldState:r,formState:n}=(0,a.Gc)(),o=r(e.name,n);if(!e)throw Error("useFormField should be used within <FormField>");let{id:i}=t;return{id:i,name:e.name,formItemId:"".concat(i,"-form-item"),formDescriptionId:"".concat(i,"-form-item-description"),formMessageId:"".concat(i,"-form-item-message"),...o}},h=s.createContext({}),p=s.forwardRef((e,t)=>{let{className:r,...o}=e,a=s.useId();return(0,n.jsx)(h.Provider,{value:{id:a},children:(0,n.jsx)("div",{ref:t,className:(0,i.cn)("space-y-2",r),...o})})});p.displayName="FormItem";let v=s.forwardRef((e,t)=>{let{className:r,...s}=e,{error:o,formItemId:a}=x();return(0,n.jsx)(d,{ref:t,className:(0,i.cn)(o&&"text-destructive",r),htmlFor:a,...s})});v.displayName="FormLabel";let g=s.forwardRef((e,t)=>{let{...r}=e,{error:s,formItemId:a,formDescriptionId:i,formMessageId:l}=x();return(0,n.jsx)(o.g7,{ref:t,id:a,"aria-describedby":s?"".concat(i," ").concat(l):"".concat(i),"aria-invalid":!!s,...r})});g.displayName="FormControl";let j=s.forwardRef((e,t)=>{let{className:r,...s}=e,{formDescriptionId:o}=x();return(0,n.jsx)("p",{ref:t,id:o,className:(0,i.cn)("text-[0.8rem] text-muted-foreground",r),...s})});j.displayName="FormDescription";let b=s.forwardRef((e,t)=>{let{className:r,children:s,...o}=e,{error:a,formMessageId:l}=x(),c=a?String(null==a?void 0:a.message):s;return c?(0,n.jsx)("p",{ref:t,id:l,className:(0,i.cn)("text-[0.8rem] font-medium text-destructive",r),...o,children:c}):null});b.displayName="FormMessage"},3335:function(e,t,r){"use strict";r.d(t,{I:function(){return a}});var n=r(881),s=r(4149),o=r(270);let a=s.forwardRef((e,t)=>{let{className:r,type:s,...a}=e;return(0,n.jsx)("input",{type:s,className:(0,o.cn)("flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",r),ref:t,...a})});a.displayName="Input"},8766:function(e,t,r){"use strict";r.d(t,{O:function(){return o}});var n=r(881),s=r(270);function o(e){let{className:t,...r}=e;return(0,n.jsx)("div",{className:(0,s.cn)("animate-pulse rounded-md bg-primary/10",t),...r})}},7126:function(e,t,r){"use strict";r.d(t,{h:function(){return s}});var n=r(7597);let s=function(e){let{baseUrl:t}=e,r=[],n=[],s=async function(e){let s=arguments.length>1&&void 0!==arguments[1]?arguments[1]:{},o="".concat(t).concat(e),a={...s};for(let e of r)a=e(a);let i=await fetch(o,a);for(let e of n){let t=await e(i);if("code"in t)return t;i=t}if(!i.ok)throw Error("HTTP error! status: ".concat(i.status));let l=await i.json();if(0!==l.code&&200!==l.code)throw Error("API error! code: ".concat(l.code,", message: ").concat(l.msg));return l};return{request:s,get:(e,t)=>s(e,{...t,method:"GET"}),post:(e,t,r)=>s(e,{...r,method:"POST",body:JSON.stringify(t),headers:{"Content-Type":"application/json",...null==r?void 0:r.headers}}),delete:(e,t)=>s(e,{...t,method:"DELETE"}),put:(e,t,r)=>s(e,{...r,method:"PUT",body:JSON.stringify(t),headers:{"Content-Type":"application/json",...null==r?void 0:r.headers}}),addRequestInterceptor:e=>{r.push(e)},addResponseInterceptor:e=>{n.push(e)}}}({baseUrl:r(8544).env.NEXT_PUBLIC_API_BASE_URL||"/v1"});s.addRequestInterceptor(e=>{let t=localStorage.getItem("token");return t?{...e,headers:{...e.headers,Authorization:"Bearer ".concat(t)}}:e}),s.addResponseInterceptor(async e=>{if(401===e.status)return e;let t=await e.clone().json();return 0!==t.code&&200!==t.code?(n.Am.error(t.msg),Promise.reject(t.msg)):e})},270:function(e,t,r){"use strict";r.d(t,{cn:function(){return o}});var n=r(3958),s=r(4399);function o(){for(var e=arguments.length,t=Array(e),r=0;r<e;r++)t[r]=arguments[r];return(0,s.m6)((0,n.W)(t))}}},function(e){e.O(0,[597,718,452,77,985,330,744],function(){return e(e.s=93)}),_N_E=e.O()}]);