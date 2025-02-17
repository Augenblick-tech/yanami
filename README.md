# 追番service

简化追番流程，自动化订阅新番更新

> 服务启动后会通过bangumi获取当前正在更新的番剧列表  
> 在tmdb上查出来对应番的季度信息和日语原名、简中翻译、繁中翻译  
> 通过配置的RSS全站订阅链接和搜索链接去轮询更新  
> 将从RSS获取到的种子资源和预设的番剧规则进行匹配，命中规则便推送qbit下载  
> 通过识别番剧的剧集和记录下载历史，来判断番剧是否更新完结，是则停止监听更新

## 1.0版本

### 功能

- [x] 增删RSS订阅
- [x] 增删预设规则
  - [x] 按优先级匹配规则
  - [x] 命中过规则则不再匹配其他规则
- [x] 增删追番规则
- [x] 用户访问权限管理
- [x] 推送qbit下载
- [x] 番剧完结自动停止

### 运行

`cargo run -- --addr 127.0.0.1:1234 --mode debug --key yanami --db-path sqlite://yanami.db?mode=rwc`

#### 启动配置

```toml
addr = "127.0.0.1:1234"
mode = "debug"
key = "your_yanami_service_auth_key"
db_path = "sqlite://yanami.db?mode=rwc"
tmdb_token = "your_tmdb_key"
```

### 接口
OpenApi: `http://127.0.0.1:1234/redoc`  
SwaggerUI: `http://127.0.0.1:1234/swagger-ui`
