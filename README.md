# 追番service

简化追番流程，自动化订阅新番更新

## 1.0版本

### 功能

- [ ] 增删RSS订阅
- [ ] 增删预设规则
- [ ] 增删追番规则
- [ ] 用户访问权限管理

### 运行

`cargo run -- --addr 127.0.0.1:1234 --mode debug --key yanami --db-path ./yanami.redb`

### 接口
OpenApi: `http://127.0.0.1:1234/redoc`
