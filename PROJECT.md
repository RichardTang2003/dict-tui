# dict-tui 项目概述

## 项目信息
- **语言**: Rust
- **版本**: 0.1.2
- **描述**: 基于 Rust 的终端词典工具，读取 Mdict (.mdx) 词典并提供实时查询界面

## 项目结构
```
dict-tui/
├── Cargo.toml              # 项目配置
├── Cargo.lock
├── README.md
├── .gitignore
├── .github/
│   └── workflows/
│       └── release.yml
└── src/
    ├── main.rs             # 程序入口
    ├── app/                # 应用层
    │   └── mod.rs          # Config 配置结构
    ├── dict/               # 词典层
    │   ├── mod.rs
    │   ├── entry.rs        # Entry 词条结构
    │   └── store.rs        # DictionaryStore 词典存储与搜索
    ├── cache/              # 缓存层
    │   ├── mod.rs
    │   ├── query_cache.rs  # QueryResultCache 搜索结果缓存
    │   └── definition_cache.rs  # DefinitionCache 词条详情缓存
    ├── ai/                 # AI 层
    │   ├── mod.rs
    │   ├── client.rs       # AiClient AI 客户端
    │   └── prompt.rs       # 提示词模板
    ├── ui/                 # UI 层
    │   └── search.rs       # 搜索界面
    └── render/             # 渲染层
        ├── mod.rs
        ├── html.rs         # HTML 转纯文本
        └── browser.rs      # 浏览器预览
```

## 模块设计

### app (应用层)
- **职责**: 配置管理
- **Config**: AI 配置结构
  - `api_endpoint`: API 端点
  - `api_key`: 密钥
  - `model`: 模型名称
  - `answer_language`: 回答语言
  - `system_prompt`: 系统提示词

### dict (词典层)
- **职责**: 词典加载、索引、搜索
- **Entry**: 词条数据结构
- **DictionaryStore**: 主数据结构
  - `entries`: 所有词条
  - `sources`: 各词典源
  - `load_from_dir()`: 递归扫描 .mdx 文件
  - `search()`: 三级匹配搜索
  - `fetch_definition()`: 获取词条定义

### cache (缓存层)
- **职责**: 查询结果缓存、词条详情缓存
- **QueryResultCache**: LRU 搜索结果缓存 (容量 2048)
- **DefinitionCache**: LRU 词条详情缓存 (容量 4096)

### ai (AI 层)
- **职责**: AI 查询、提示词管理
- **AiClient**: OpenAI 兼容 API 客户端
- **prompt.rs**: 用户提示词模板

### ui (UI 层)
- **职责**: 终端界面、用户交互
- **search.rs**: 主搜索界面 (双栏布局)

### render (渲染层)
- **职责**: HTML 处理、浏览器预览
- **html.rs**: HTML 转纯文本
- **browser.rs**: 生成预览 HTML、调用系统浏览器

## 依赖
- `anyhow`: 错误处理
- `crossterm`: 终端输入
- `ratatui`: TUI 组件
- `regex`: HTML 解析
- `rs-mdict`: Mdx 词典解析
- `url`: URL 处理
- `once_cell`: 静态初始化
- `reqwest`: HTTP 客户端
- `serde`/`serde_json`: 配置序列化
- `tokio`: 异步运行时
- `dirs`: 系统目录访问

## 快捷键
| 键 | 功能 |
|---|---|
| 字符输入 | 实时搜索 |
| Backspace | 删除字符 |
| `,` `<` 等 | 上一个词条 |
| `.` `>` 等 | 下一个词条 |
| `↑` `↓` | 滚动详情 |
| `Home` `End` | 跳转首/末条 |
| `PageUp` `PageDown` | 翻页 |
| `Ctrl+O` / `F2` | 浏览器预览 |
| `Ctrl+G` | AI 查询 |
| `Esc` | 退出 |

## 配置
- AI 配置路径: `~/.config/dict-tui/config.json`
- 词典目录: `./dictionary` (默认)

## 扩展指南

### 添加新模块
1. 在 `src/` 下创建新目录 e.g., `src/foo/`
2. 创建 `mod.rs` 导出子模块
3. 在 `main.rs` 中添加 `mod foo;`
4. 在需要的地方通过 `crate::foo::*` 引用

### 修改 AI 提示词
1. 修改 `ai/prompt.rs` 中的模板