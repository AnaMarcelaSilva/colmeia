# Dependências e licenças

O que entra no binário do núcleo (`go version -m colmeia-nucleo`), conferido pelo arquivo de licença de cada módulo. Nenhuma é GPL, LGPL ou AGPL. A tela (Rust) não ganhou dependência nova na entrega dos bancos de dados.

| Módulo | Versão | Licença | Para quê |
| --- | --- | --- | --- |
| github.com/coder/websocket | v1.8.15 | ISC | WebSocket dos terminais e dos eventos |
| github.com/creack/pty | v1.1.24 | MIT | Pseudo-terminais dos agentes |
| modernc.org/sqlite (+ libc, mathutil, memory) | v1.60.1 | BSD-3-Clause | O banco da Colmeia e as conexões SQLite |
| github.com/dustin/go-humanize, github.com/google/uuid, github.com/remyoudompheng/bigfft | | MIT, BSD-3-Clause | Dependências do modernc |
| github.com/jackc/pgx/v5 (+ pgpassfile, pgservicefile, puddle) | v5.11.0 | MIT | PostgreSQL |
| github.com/go-sql-driver/mysql | v1.10.1 | MPL-2.0 | MySQL e MariaDB (usado como dependência, sem modificação) |
| filippo.io/edwards25519 | v1.2.0 | BSD-3-Clause | Dependência do driver MySQL |
| github.com/microsoft/go-mssqldb (+ golang-sql/civil, golang-sql/sqlexp, shopspring/decimal) | v1.11.2 | BSD-3-Clause, Apache-2.0, MIT | SQL Server (o subpacote `azuread` não é importado) |
| github.com/zalando/go-keyring | v0.2.8 | MIT | Chaveiro do sistema |
| github.com/godbus/dbus/v5 | v5.2.2 | BSD-2-Clause | Secret Service pelo D-Bus (dependência do go-keyring) |
| golang.org/x/crypto, x/sync, x/sys, x/text | | BSD-3-Clause | Biblioteca estendida do Go |

Para conferir de novo:

```bash
go -C nucleo build -o /tmp/colmeia-nucleo ./cmd/colmeia-nucleo
go version -m /tmp/colmeia-nucleo        # os módulos que entram no binário
go -C nucleo list -m -f '{{.Path}} {{.Dir}}' all   # onde está o LICENSE de cada um
```
