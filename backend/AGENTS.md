# AGENTS.md

## 代码质量

- 降低代码的认知复杂度为架构要求。

## 后端命名规范

- 变量命名应避免语义晦涩的缩写(如语焉不详的`h`,`td`), 约定俗成的循环下标除外(`i`,`j`,etc.)
- 变量命名应清晰表达作用，如用 `err` 捕获产生的错误，用 `server` 创建服务，用`supportImage`判断是否支持图像等。允许使用常见缩写(`err`,`cnt`,`msg`,`dir`,`ID`)、单词(`name`,`text`)或短语(`supportImage`)。
- 后端向 extension 暴露的接口统一使用 `ex_` / `Ex_` 前缀，以便与其他接口区分。按 Go 的可见性规则，包外可访问的标识符使用 `Ex_`，仅包内可见的使用 `ex_`。

## 编译打包 

- Do not compile or launch the backend binary directly with `go build` or `go run`.
