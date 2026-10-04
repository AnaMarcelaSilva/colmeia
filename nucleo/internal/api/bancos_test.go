//go:build unix

package api

import (
	"bytes"
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/coder/websocket"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/chaveiro"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

// lojaSQLite cria um SQLite com pedidos e clientes.
func lojaSQLite(t *testing.T, pedidos int) string {
	t.Helper()
	arquivo := filepath.Join(t.TempDir(), "loja.db")
	db, err := sql.Open("sqlite", arquivo)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	for _, c := range []string{
		`CREATE TABLE clientes (id INTEGER PRIMARY KEY, nome TEXT)`,
		`CREATE TABLE pedidos (id INTEGER PRIMARY KEY, cliente_id INTEGER REFERENCES clientes(id), total REAL)`,
		`INSERT INTO clientes (nome) VALUES ('cliente-x')`,
	} {
		if _, err := db.Exec(c); err != nil {
			t.Fatal(err)
		}
	}
	tx, _ := db.Begin()
	for i := 1; i <= pedidos; i++ {
		tx.Exec(`INSERT INTO pedidos (cliente_id, total) VALUES (1, ?)`, float64(i))
	}
	tx.Commit()
	return arquivo
}

const canario = "canario-7f3a-SENHA"

func TestSenhaNuncaSaiDoChaveiro(t *testing.T) {
	var registro bytes.Buffer
	log.SetOutput(&registro)
	t.Cleanup(func() { log.SetOutput(os.Stderr) })
	srv, servidor := servidorComAgentes(t)
	cofre := chaveiro.NovaMemoria()
	servidor.Chaveiro = cofre
	montarTarefas(t, srv.URL)
	eventos := ouvirComToken(t, srv.URL, "1")
	var respostas []string
	guardar := func(status int, r map[string]any) map[string]any {
		b, _ := json.Marshal(r)
		respostas = append(respostas, string(b))
		return r
	}
	// Um servidor que não existe (porta 1): os erros também não podem levar a senha.
	status, c := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/perfis/1/conexoes", map[string]any{"nome": "loja-web-dev", "tipo": "postgres",
		"host": "127.0.0.1", "porta": 1, "usuario": "app", "banco": "loja", "senha": canario, "guardar": true})
	guardar(status, c)
	if status != http.StatusOK || c["senha"] != "chaveiro" || c["senha_disponivel"] != true {
		t.Fatalf("criar: %d %v", status, c)
	}
	if contas := cofre.Contas(); len(contas) != 1 || !strings.HasPrefix(contas[0], "conexao:") {
		t.Fatalf("chaveiro: %v", contas)
	}
	guardar(pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/conexoes", nil))
	guardar(pedirComo(t, tokenTela, "GET", srv.URL+"/v1/conexoes/1", nil))
	status, teste := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/conexoes/1/testar", nil)
	guardar(status, teste)
	if teste["ok"] != false || teste["erro"] != "Não achou o servidor 127.0.0.1:1." {
		t.Fatalf("testar: %v", teste)
	}
	guardar(pedirComo(t, tokenTela, "POST", srv.URL+"/v1/perfis/1/conexoes/testar", map[string]any{"nome": "x", "tipo": "mysql", "host": "127.0.0.1",
		"porta": 1, "usuario": "app", "senha": canario}))
	guardar(pedirComo(t, tokenTela, "GET", srv.URL+"/v1/conexoes/1/arvore?nivel=bancos", nil))
	guardar(pedirComo(t, tokenTela, "POST", srv.URL+"/v1/conexoes/1/execucoes", map[string]any{"ficha": "ficha-canario-00001", "sql": "SELECT 1"}))
	guardar(pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/conexoes/1", map[string]any{"nome": "loja-web-dev", "tipo": "postgres", "host": "127.0.0.1",
		"porta": 2, "usuario": "app", "senha": canario + "-2", "guardar": true}))
	for _, r := range respostas {
		if strings.Contains(r, canario) {
			t.Fatalf("a senha apareceu numa resposta: %s", r)
		}
	}
	// Nem no banco da Colmeia (inclusive o WAL), nem no log, nem nos eventos.
	for _, sufixo := range []string{"", "-wal"} {
		bruto, _ := os.ReadFile(filepath.Join(servidor.DirDados, "dados", "colmeia.db"+sufixo))
		if bytes.Contains(bruto, []byte(canario)) {
			t.Fatalf("a senha está no colmeia.db%s", sufixo)
		}
	}
	if strings.Contains(registro.String(), canario) {
		t.Fatal("a senha apareceu no log")
	}
	eventos.esperar("conexão alterada", func(m map[string]any) bool { return m["tipo"] == "conexao.mudou" && m["acao"] == "alterada" })
	for _, m := range eventos.todas() {
		if strings.Contains(m, canario) {
			t.Fatalf("a senha apareceu num aviso: %s", m)
		}
	}
	// Esquecer: sai do chaveiro e a próxima conexão pergunta (428).
	pedirComo(t, tokenTela, "DELETE", srv.URL+"/v1/conexoes/1/senha", nil)
	if len(cofre.Contas()) != 0 {
		t.Fatal("esquecer deveria tirar do chaveiro")
	}
	if status, r := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/conexoes/1/arvore?nivel=bancos", nil); status != http.StatusPreconditionRequired || r["precisa_senha"] != true {
		t.Fatalf("sem senha: %d %v", status, r)
	}
	// Sem chaveiro e sem pedir para guardar, a senha fica só na memória.
	servidor.Chaveiro = &chaveiro.Memoria{Ausente: true}
	status, r := pedirComo(t, tokenTela, "PUT", srv.URL+"/v1/conexoes/1/senha", map[string]any{"senha": canario, "guardar": false})
	if status != http.StatusOK || r["senha"] != "memoria" {
		t.Fatalf("sem chaveiro: %d %v", status, r)
	}
	if _, lista := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/conexoes", nil); lista["chaveiro_disponivel"] != false {
		t.Fatalf("lista sem chaveiro: %v", lista)
	}
	// Remover apaga do chaveiro.
	servidor.Chaveiro = cofre
	pedirComo(t, tokenTela, "PUT", srv.URL+"/v1/conexoes/1/senha", map[string]any{"senha": canario, "guardar": true})
	pedirComo(t, tokenTela, "DELETE", srv.URL+"/v1/conexoes/1", nil)
	if len(cofre.Contas()) != 0 {
		t.Fatal("remover deveria tirar do chaveiro")
	}
}

// Um momento sem chaveiro não muda a conexão de lugar: ela continua do
// chaveiro, a senha digitada vale só para a sessão, e quando o chaveiro volta
// a senha guardada nele é usada de novo.
func TestFaltaDeChaveiroEhDaSessao(t *testing.T) {
	srv, servidor := servidorComAgentes(t)
	cofre := chaveiro.NovaMemoria()
	servidor.Chaveiro = cofre
	montarTarefas(t, srv.URL)
	status, c := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/perfis/1/conexoes", map[string]any{"nome": "loja-web-dev", "tipo": "postgres",
		"host": "127.0.0.1", "porta": 1, "usuario": "app", "banco": "loja", "senha": canario, "guardar": true})
	if status != http.StatusOK || c["senha"] != "chaveiro" {
		t.Fatalf("criar: %d %v", status, c)
	}
	// "Reinicia" sem chaveiro: a memória do núcleo some e o chaveiro não responde.
	servidor.Chaveiro = &chaveiro.Memoria{Ausente: true}
	servidor.esquecerDaMemoria(1)
	if _, l := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/conexoes/1", nil); l["senha_disponivel"] != false || l["senha"] != "chaveiro" {
		t.Fatalf("sem chaveiro, a senha não está à mão: %v", l)
	}
	// A tela, sem chaveiro, manda guardar=false ("Lembrar até fechar a Colmeia").
	status, r := pedirComo(t, tokenTela, "PUT", srv.URL+"/v1/conexoes/1/senha", map[string]any{"senha": canario, "guardar": false})
	if status != http.StatusOK || r["senha"] != "chaveiro" {
		t.Fatalf("a conexão deveria continuar do chaveiro: %d %v", status, r)
	}
	// O chaveiro volta (núcleo novo): a senha antiga está lá e é usada.
	servidor.Chaveiro = cofre
	servidor.esquecerDaMemoria(1)
	if _, l := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/conexoes/1", nil); l["senha_disponivel"] != true || l["senha"] != "chaveiro" {
		t.Fatalf("com o chaveiro de volta: %v", l)
	}
	if v, err := servidor.senhaDe(dadosConexao(t, servidor, 1)); err != nil || v != canario {
		t.Fatalf("senha do chaveiro: %q %v", v, err)
	}
}

func TestConsoleComSQLite(t *testing.T) {
	srv, _ := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	arquivo := lojaSQLite(t, 1200)
	status, c := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/perfis/1/conexoes", map[string]any{"nome": "loja-local", "tipo": "sqlite", "arquivo": arquivo})
	if status != http.StatusOK || c["senha"] != "nenhuma" {
		t.Fatalf("criar: %d %v", status, c)
	}
	url := srv.URL + "/v1/conexoes/1"
	executar := func(corpo map[string]any) (int, map[string]any) {
		if corpo["ficha"] == nil {
			corpo["ficha"] = fmt.Sprintf("ficha-de-teste-%06d", time.Now().UnixNano()%1000000)
		}
		return pedirComo(t, tokenTela, "POST", url+"/execucoes", corpo)
	}
	status, r := executar(map[string]any{"ficha": "ficha-pedidos-00001", "sql": "SELECT * FROM pedidos ORDER BY id", "limite": 500})
	if linhas, _ := r["linhas"].([]any); status != http.StatusOK || len(linhas) != 500 || r["mais"] != true {
		t.Fatalf("select: %d %v", status, r["erro"])
	}
	status, r = pedirComo(t, tokenTela, "GET", srv.URL+"/v1/execucoes/ficha-pedidos-00001/mais?limite=1000", nil)
	if linhas, _ := r["linhas"].([]any); status != http.StatusOK || len(linhas) != 700 || r["mais"] != false {
		t.Fatalf("carregar mais: %d %v", status, r["erro"])
	}
	if status, _ := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/execucoes/ficha-pedidos-00001/mais", nil); status != http.StatusGone {
		t.Fatalf("depois do fim: %d", status)
	}
	// Somente leitura por padrão.
	if status, r := executar(map[string]any{"sql": "DELETE FROM pedidos"}); status != http.StatusForbidden || r["somente_leitura"] != true {
		t.Fatalf("delete em leitura: %d %v", status, r)
	}
	if status, r := executar(map[string]any{"sql": "SELECT 1; SELECT 2"}); status != http.StatusBadRequest || r["varias"] != true {
		t.Fatalf("várias: %d %v", status, r)
	}
	status, r = executar(map[string]any{"sql": "SELECT\n*\nFROM nao_existe"})
	if status != http.StatusUnprocessableEntity || !strings.Contains(fmt.Sprint(r["erro"]), "nao_existe") {
		t.Fatalf("erro de SQL: %d %v", status, r)
	}
	// Liga a escrita: a alteração pede confirmação, uma vez, presa ao SQL.
	pedirComo(t, tokenTela, "PATCH", url, map[string]any{"nome": "loja-local", "tipo": "sqlite", "arquivo": arquivo, "escrita": true})
	status, r = executar(map[string]any{"sql": "UPDATE pedidos SET total = 0"})
	if status != http.StatusConflict || r["precisa_confirmar"] != true || r["verbo"] != "UPDATE" || r["sem_where"] != true || r["confirmacao"] == "" {
		t.Fatalf("pedir confirmação: %d %v", status, r)
	}
	nonce := r["confirmacao"]
	if status, r := executar(map[string]any{"sql": "DELETE FROM pedidos", "confirmar": nonce}); status != http.StatusConflict || r["confirmacao_invalida"] != true {
		t.Fatalf("confirmação de outra instrução: %d %v", status, r)
	}
	status, r = executar(map[string]any{"sql": "UPDATE pedidos SET total = 0"})
	nonce = r["confirmacao"]
	status, r = executar(map[string]any{"sql": "UPDATE pedidos SET total = 0", "confirmar": nonce})
	if status != http.StatusOK || r["afetadas"] != 1200.0 {
		t.Fatalf("alteração confirmada: %d %v", status, r)
	}
	if status, _ := executar(map[string]any{"sql": "UPDATE pedidos SET total = 0", "confirmar": nonce}); status != http.StatusConflict {
		t.Fatalf("a mesma confirmação de novo: %d", status)
	}
	// Prévia, árvore e histórico.
	status, p := pedirComo(t, tokenTela, "POST", url+"/previa", map[string]any{"objeto": "pedidos"})
	if resultado, _ := p["resultado"].(map[string]any); status != http.StatusOK || len(resultado["linhas"].([]any)) != 100 {
		t.Fatalf("prévia: %d %v", status, p)
	}
	status, a := pedirComo(t, tokenTela, "GET", url+"/arvore?nivel=objetos", nil)
	if tabelas, _ := a["tabelas"].([]any); status != http.StatusOK || len(tabelas) != 2 || a["total"] != 2.0 {
		t.Fatalf("árvore: %d %v", status, a)
	}
	status, a = pedirComo(t, tokenTela, "GET", url+"/arvore?nivel=colunas&objeto=pedidos", nil)
	if colunas, _ := a["colunas"].([]any); status != http.StatusOK || len(colunas) != 3 {
		t.Fatalf("colunas: %d %v", status, a)
	}
	_, h := pedirComo(t, tokenTela, "GET", url+"/historico", nil)
	consultas, _ := h["consultas"].([]any)
	if len(consultas) != 3 || consultas[0].(map[string]any)["sql"] != "UPDATE pedidos SET total = 0" || consultas[0].(map[string]any)["altera"] != true {
		t.Fatalf("histórico: %v", consultas)
	}
	pedirComo(t, tokenTela, "DELETE", url+"/historico", nil)
	if _, h := pedirComo(t, tokenTela, "GET", url+"/historico", nil); len(h["consultas"].([]any)) != 0 {
		t.Fatal("histórico limpo")
	}
	// Na linha do tempo: consultas juntas por conexão e dia, e a alteração.
	_, linha := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/linha-do-tempo", nil)
	textos := fmt.Sprint(linha)
	if !strings.Contains(textos, "Consultou o banco loja-local") || !strings.Contains(textos, "Alterou o banco loja-local: UPDATE, 1200 linhas") {
		t.Fatalf("linha do tempo: %s", textos)
	}
}

func TestCancelarConsultaPelaTela(t *testing.T) {
	srv, _ := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/perfis/1/conexoes", map[string]any{"nome": "loja", "tipo": "sqlite", "arquivo": lojaSQLite(t, 1)})
	pesada := `WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 500000000) SELECT count(*) FROM n`
	pronto := make(chan map[string]any)
	go func() {
		_, r := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/conexoes/1/execucoes", map[string]any{"ficha": "ficha-cancelar-0001", "sql": pesada, "tempo_s": 60})
		pronto <- r
	}()
	esperarAte(t, "a execução começar", func() bool {
		_, r := pedirComo(t, tokenTela, "DELETE", srv.URL+"/v1/execucoes/ficha-cancelar-0001", nil)
		return r["ok"] == true
	})
	select {
	case r := <-pronto:
		if r["cancelada"] != true {
			t.Fatalf("cancelada: %v", r)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("o cancelamento não parou a consulta")
	}
	// No histórico, a cancelada aparece como cancelada.
	_, h := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/conexoes/1/historico", nil)
	if consultas, _ := h["consultas"].([]any); len(consultas) != 1 || consultas[0].(map[string]any)["resultado"] != "cancelada" {
		t.Fatalf("histórico da cancelada: %v", h)
	}
	_, r := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/conexoes/1/execucoes", map[string]any{"ficha": "ficha-tempo-000001", "sql": pesada, "tempo_s": 1})
	if r["tempo_esgotado"] != true || r["erro"] != "Passou de 1 s e foi cancelada." {
		t.Fatalf("tempo-limite: %v", r)
	}
}

// claudeQueEscreve: um "claude" com spinner, que escreve sem parar (como o
// Claude Code esperando uma ferramenta).
func claudeQueEscreve(t *testing.T) {
	t.Helper()
	bin := t.TempDir()
	script := "#!/bin/sh\necho pronto\nwhile true; do printf '. '; sleep 0.05; done\n"
	if err := os.WriteFile(filepath.Join(bin, "claude"), []byte(script), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", bin+":"+os.Getenv("PATH"))
	t.Setenv("CLAUDE_CONFIG_DIR", t.TempDir())
}

func TestAgenteConsultaComAprovacao(t *testing.T) {
	claudeQueEscreve(t)
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	arquivo := lojaSQLite(t, 300)
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/perfis/1/conexoes", map[string]any{"nome": "fechada", "tipo": "sqlite", "arquivo": arquivo})
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/perfis/1/conexoes", map[string]any{"nome": "loja-web-dev", "tipo": "sqlite", "arquivo": arquivo,
		"agentes": true, "escrita": true})
	eventos := ouvirComToken(t, srv.URL, "1")
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	token := ""
	esperarAte(t, "o token do agente", func() bool {
		token = lerArquivo(filepath.Join(servidor.DirCanal, "agentes", "1.token"))
		return token != ""
	})
	eventos.esperar("tarefa trabalhando", coluna("tarefa.atualizada", "trabalhando", "automatico"))

	// Só a conexão liberada aparece, sem host nem usuário.
	_, lista := pedirComo(t, token, "GET", srv.URL+"/v1/agente/bancos", nil)
	if conexoes := lista["conexoes"].([]any); len(conexoes) != 1 || conexoes[0].(map[string]any)["nome"] != "loja-web-dev" {
		t.Fatalf("listar: %v", lista)
	}
	if status, _ := pedirComo(t, token, "POST", srv.URL+"/v1/agente/bancos/1/consultas", map[string]any{"sql": "SELECT 1"}); status != http.StatusBadRequest {
		t.Fatalf("conexão fechada para agentes: %d", status)
	}
	// Escrita, mesmo com escrita ligada na conexão: recusada sem pedir nada.
	if status, r := pedirComo(t, token, "POST", srv.URL+"/v1/agente/bancos/2/consultas", map[string]any{"sql": "DELETE FROM pedidos"}); status != http.StatusBadRequest ||
		!strings.Contains(fmt.Sprint(r["erro"]), "só consulta") {
		t.Fatalf("escrita do agente: %d %v", status, r)
	}
	if status, _ := pedirComo(t, token, "POST", srv.URL+"/v1/agente/bancos/2/consultas", map[string]any{"sql": "SELECT load_extension('x')"}); status != http.StatusBadRequest {
		t.Fatalf("função negada: %d", status)
	}

	consultar := func(sql string) chan map[string]any {
		pronto := make(chan map[string]any, 1)
		go func() {
			status, r := pedirComo(t, token, "POST", srv.URL+"/v1/agente/bancos/2/consultas", map[string]any{"sql": sql})
			r["_status"] = float64(status)
			pronto <- r
		}()
		return pronto
	}
	// Aprovar: o cartão espera você, o agente recebe o resultado.
	pronto := consultar("SELECT id, total FROM pedidos ORDER BY id")
	pedida := eventos.esperar("pedido de aprovação", func(m map[string]any) bool { return m["tipo"] == "banco.aprovacao" && m["acao"] == "pedida" })
	a := pedida["aprovacao"].(map[string]any)
	if a["sql"] != "SELECT id, total FROM pedidos ORDER BY id" || a["agente"] != "Claude Code (dev)" || a["conexao"] != "loja-web-dev" || a["expira_hora"] == "" {
		t.Fatalf("pedido: %v", a)
	}
	eventos.esperar("agente aguardando a aprovação", func(m map[string]any) bool {
		return m["tipo"] == "agente.estado" && m["estado"] == "aguardando" && m["motivo"] == "aprovar consulta"
	})
	eventos.esperar("cartão em Aguardando você", coluna("tarefa.atualizada", "aguardando", "automatico"))
	// O spinner continua escrevendo e o estado não muda.
	time.Sleep(400 * time.Millisecond)
	if sessao, _ := servidor.Agentes.Pegar(1); sessao != nil {
		if estado, motivo, _ := sessao.Estado(); estado != "aguardando" || motivo != "aprovar consulta" {
			t.Fatalf("segurado: %s %s", estado, motivo)
		}
	}
	_, q := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/quadro", nil)
	if pendentes, _ := q["aprovacoes"].([]any); len(pendentes) != 1 {
		t.Fatalf("aprovações no quadro: %v", q["aprovacoes"])
	}
	if status, _ := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/aprovacoes/"+a["id"].(string), map[string]any{"aprovar": true}); status != http.StatusOK {
		t.Fatalf("aprovar: %d", status)
	}
	r := <-pronto
	if r["_status"] != 200.0 || r["linhas"] != 200.0 || r["mais"] != true || !strings.Contains(r["texto"].(string), "id | total\n1 | 1\n") {
		t.Fatalf("resultado ao agente: %v", r)
	}
	viu := [2]bool{}
	eventos.esperar("resolvida e o agente trabalhando de novo", func(m map[string]any) bool {
		viu[0] = viu[0] || (m["tipo"] == "banco.aprovacao" && m["acao"] == "resolvida" && m["resultado"] == "aprovada" && m["linhas"] == 200.0)
		viu[1] = viu[1] || (m["tipo"] == "agente.estado" && m["estado"] == "trabalhando")
		return viu[0] && viu[1]
	})

	// Recusar com motivo: vai só para o agente.
	pronto = consultar("SELECT * FROM clientes")
	pedida = eventos.esperar("segundo pedido", func(m map[string]any) bool {
		a, _ := m["aprovacao"].(map[string]any)
		return m["tipo"] == "banco.aprovacao" && m["acao"] == "pedida" && a["sql"] == "SELECT * FROM clientes"
	})
	id := pedida["aprovacao"].(map[string]any)["id"].(string)
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/aprovacoes/"+id, map[string]any{"aprovar": false, "motivo": "use a tabela pedidos"})
	r = <-pronto
	if r["_status"] != 409.0 || !strings.Contains(r["erro"].(string), "use a tabela pedidos") {
		t.Fatalf("recusa: %v", r)
	}
	if status, _ := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/aprovacoes/"+id, map[string]any{"aprovar": true}); status != http.StatusBadRequest {
		t.Fatal("responder de novo um pedido resolvido")
	}

	// Sem resposta no prazo: expira e conta como recusa.
	servidor.PrazoAprovacao = 300 * time.Millisecond
	r = <-consultar("SELECT 1")
	if r["_status"] != 409.0 || r["resultado"] != "expirou" {
		t.Fatalf("expirou: %v", r)
	}
	// Na linha do tempo, sem o SQL.
	_, linha := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/linha-do-tempo", nil)
	textos := fmt.Sprint(linha)
	for _, esperado := range []string{"Claude Code (dev) consultou o banco loja-web-dev (aprovado por você)", "Você recusou uma consulta do Claude Code (dev) no banco loja-web-dev",
		"Uma consulta do Claude Code (dev) no banco loja-web-dev expirou sem resposta"} {
		if !strings.Contains(textos, esperado) {
			t.Errorf("falta na linha do tempo: %q em %s", esperado, textos)
		}
	}
	if strings.Contains(textos, "FROM clientes") {
		t.Fatal("o SQL foi para a linha do tempo")
	}

	// O agente removido no meio da espera: o pedido some e nada fica preso.
	servidor.PrazoAprovacao = time.Minute
	pronto = consultar("SELECT 2")
	eventos.esperar("pedido antes de remover", func(m map[string]any) bool {
		a, _ := m["aprovacao"].(map[string]any)
		return m["tipo"] == "banco.aprovacao" && m["acao"] == "pedida" && a["sql"] == "SELECT 2"
	})
	pedirComo(t, tokenTela, "DELETE", srv.URL+"/v1/agentes/1", nil)
	// O processo do MCP morre com o agente; aqui o pedido continua até o
	// cliente desistir: o token não vale mais, mas o pedido já entrou.
	esperarAte(t, "o agente sair", func() bool { _, ok := servidor.Agentes.Pegar(1); return !ok || servidor.Agentes.Quantidade() == 0 })
	servidor.cancelarAprovacoes()
	if r := <-pronto; r["_status"] != 409.0 {
		t.Fatalf("pedido do agente removido: %v", r)
	}
}

// ouvinteComToken lê o WebSocket de eventos pelo canal com token e guarda
// tudo o que chegou.
type ouvinteComToken struct {
	t      *testing.T
	conn   *websocket.Conn
	ctx    context.Context
	mu     sync.Mutex
	brutas []string
}

func ouvirComToken(t *testing.T, url, perfil string) *ouvinteComToken {
	t.Helper()
	ctx, cancelar := context.WithTimeout(context.Background(), 30*time.Second)
	t.Cleanup(cancelar)
	conn, _, err := websocket.Dial(ctx, "ws"+strings.TrimPrefix(url, "http")+"/v1/perfis/"+perfil+"/eventos",
		&websocket.DialOptions{HTTPHeader: http.Header{"Authorization": []string{"Bearer " + tokenTela}}})
	if err != nil {
		t.Fatal(err)
	}
	conn.SetReadLimit(4 << 20)
	t.Cleanup(func() { conn.CloseNow() })
	return &ouvinteComToken{t: t, conn: conn, ctx: ctx}
}

func (o *ouvinteComToken) esperar(descricao string, ok func(map[string]any) bool) map[string]any {
	o.t.Helper()
	for {
		_, bruto, err := o.conn.Read(o.ctx)
		if err != nil {
			o.t.Fatalf("esperando %s: %v", descricao, err)
		}
		o.mu.Lock()
		o.brutas = append(o.brutas, string(bruto))
		o.mu.Unlock()
		var m map[string]any
		json.Unmarshal(bruto, &m)
		if ok(m) {
			return m
		}
	}
}

func (o *ouvinteComToken) todas() []string {
	o.mu.Lock()
	defer o.mu.Unlock()
	return append([]string(nil), o.brutas...)
}

func dadosConexao(t *testing.T, servidor *Servidor, id int64) dados.ConexaoBanco {
	t.Helper()
	c, err := servidor.Banco.ConexaoBanco(context.Background(), id)
	if err != nil {
		t.Fatal(err)
	}
	return c
}
