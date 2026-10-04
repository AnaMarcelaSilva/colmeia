package dados

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"path/filepath"
	"strings"
	"testing"
)

// tarefaComAgente monta perfil, projeto, tarefa e um agente.
func tarefaComAgente(t *testing.T, b *Banco) (Tarefa, Agente) {
	t.Helper()
	ctx := context.Background()
	perfil, _ := b.CriarPerfil(ctx, "Pessoal", "")
	ws, _ := b.CriarWorkspace(ctx, perfil.ID, "W")
	projeto, err := b.CriarProjeto(ctx, ws.ID, "loja-web", "/tmp/loja-web", "git", "main")
	if err != nil {
		t.Fatal(err)
	}
	tarefa, err := b.CriarTarefa(ctx, projeto.ID, "Nova tela de pedidos", "main", "")
	if err != nil {
		t.Fatal(err)
	}
	agente, err := b.CriarAgente(ctx, tarefa.ID, "claude", "dev", "")
	if err != nil {
		t.Fatal(err)
	}
	return tarefa, agente
}

func TestHistoricoDeMensagens(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	_, agente := tarefaComAgente(t, b)

	for _, texto := range []string{"roda os testes", "agora corrige o filtro\ne o desconto", "agora corrige o filtro\ne o desconto"} {
		if guardada, err := b.GuardarMensagem(ctx, agente.ID, texto); err != nil || !guardada {
			t.Fatalf("guardar %q: %v, %v", texto, guardada, err)
		}
	}
	lista, err := b.ListarMensagens(ctx, agente.ID)
	if err != nil {
		t.Fatal(err)
	}
	// A repetição seguida não entra; a lista vem da mais nova para a mais antiga.
	if len(lista) != 2 || lista[0].Texto != "agora corrige o filtro\ne o desconto" || lista[1].Texto != "roda os testes" {
		t.Errorf("histórico: %+v", lista)
	}

	// Segredo: não guarda e não dá erro (a mensagem foi enviada pelo terminal).
	if guardada, err := b.GuardarMensagem(ctx, agente.ID, "usa a chave sk-ant-api03-AbCdEfGhIjKlMnOpQrStUv"); err != nil || guardada {
		t.Errorf("segredo: guardada=%v, %v", guardada, err)
	}
	if lista, _ := b.ListarMensagens(ctx, agente.ID); len(lista) != 2 {
		t.Errorf("o segredo entrou no histórico: %+v", lista)
	}

	for _, ruim := range []string{"", "   \n", "com \x1b[31mcor", strings.Repeat("a", MaxTextoMensagem+1), "\xff\xfe"} {
		if _, err := b.GuardarMensagem(ctx, agente.ID, ruim); !errors.As(err, &ErrInvalido{}) {
			t.Errorf("texto inválido %q: %v", ruim[:min(len(ruim), 10)], err)
		}
	}
	if _, err := b.GuardarMensagem(ctx, 999, "oi"); !errors.Is(err, ErrNaoEncontrado) {
		t.Errorf("agente que não existe: %v", err)
	}
	if _, err := b.ListarMensagens(ctx, 999); !errors.Is(err, ErrNaoEncontrado) {
		t.Errorf("listar de agente que não existe: %v", err)
	}
}

func TestHistoricoLimitado(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	_, agente := tarefaComAgente(t, b)
	for i := range MaxMensagens + 15 {
		if _, err := b.GuardarMensagem(ctx, agente.ID, fmt.Sprintf("mensagem %d", i)); err != nil {
			t.Fatal(err)
		}
	}
	lista, _ := b.ListarMensagens(ctx, agente.ID)
	if len(lista) != MaxMensagens || lista[0].Texto != fmt.Sprintf("mensagem %d", MaxMensagens+14) || lista[len(lista)-1].Texto != "mensagem 15" {
		t.Errorf("limite: %d mensagens, de %q a %q", len(lista), lista[0].Texto, lista[len(lista)-1].Texto)
	}
	apagadas, err := b.LimparMensagens(ctx, agente.ID)
	if err != nil || apagadas != MaxMensagens {
		t.Errorf("limpar: %d, %v", apagadas, err)
	}
	if lista, _ := b.ListarMensagens(ctx, agente.ID); len(lista) != 0 {
		t.Errorf("sobrou depois de limpar: %d", len(lista))
	}
}

func TestHistoricoSaiComOAgente(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	_, agente := tarefaComAgente(t, b)
	b.GuardarMensagem(ctx, agente.ID, "oi")
	if err := b.RemoverAgente(ctx, agente.ID); err != nil {
		t.Fatal(err)
	}
	var n int
	b.db.QueryRow(`SELECT COUNT(*) FROM mensagens`).Scan(&n)
	if n != 0 {
		t.Errorf("%d mensagens ficaram depois de remover o agente", n)
	}
	// Nada do histórico entra nos eventos.
	var eventos int
	b.db.QueryRow(`SELECT COUNT(*) FROM eventos WHERE dados LIKE '%oi%'`).Scan(&eventos)
	if eventos != 0 {
		t.Error("o texto da mensagem entrou no histórico de eventos")
	}
}

func TestNotas(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	tarefa, _ := tarefaComAgente(t, b)

	if _, err := b.DefinirNota(ctx, tarefa.ID, "daily", "2026-10-02", "Mostrar a tela nova\n"); err != nil {
		t.Fatal(err)
	}
	if _, err := b.DefinirNota(ctx, tarefa.ID, "daily", "2026-10-02", "Mostrar a tela nova e o filtro"); err != nil {
		t.Fatal(err)
	}
	if _, err := b.DefinirNota(ctx, tarefa.ID, "sprint", "2026-09-21..2026-10-02", "Entregue"); err != nil {
		t.Fatal(err)
	}
	notas, _ := b.NotasDoPeriodo(ctx, []int64{tarefa.ID, 99}, "daily", "2026-10-02")
	if len(notas) != 1 || notas[tarefa.ID].Texto != "Mostrar a tela nova e o filtro" {
		t.Errorf("notas da daily: %+v", notas)
	}
	ultimas, _ := b.UltimasNotas(ctx, []int64{tarefa.ID}, "sprint")
	if ultimas[tarefa.ID].Periodo != "2026-09-21..2026-10-02" {
		t.Errorf("última nota de sprint: %+v", ultimas)
	}
	// O evento leva o tamanho, nunca o texto.
	var conteudo string
	b.db.QueryRow(`SELECT dados FROM eventos WHERE tipo = 'nota.atualizada' ORDER BY id DESC LIMIT 1`).Scan(&conteudo)
	if strings.Contains(conteudo, "Entregue") || !strings.Contains(conteudo, `"tamanho":8`) {
		t.Errorf("evento da nota: %s", conteudo)
	}
	// Texto vazio apaga.
	if _, err := b.DefinirNota(ctx, tarefa.ID, "daily", "2026-10-02", ""); err != nil {
		t.Fatal(err)
	}
	if notas, _ := b.NotasDoPeriodo(ctx, []int64{tarefa.ID}, "daily", "2026-10-02"); len(notas) != 0 {
		t.Errorf("a nota apagada ficou: %+v", notas)
	}

	for _, c := range []struct{ tipo, periodo string }{
		{"daily", "2026-02-30"}, {"daily", "ontem"}, {"daily", "2026-10-02..2026-10-03"},
		{"sprint", "2026-10-02"}, {"sprint", "2026-10-05..2026-10-01"}, {"semana", "2026-10-02"},
	} {
		if _, err := b.DefinirNota(ctx, tarefa.ID, c.tipo, c.periodo, "x"); !errors.As(err, &ErrInvalido{}) {
			t.Errorf("período %s %q aceito: %v", c.tipo, c.periodo, err)
		}
	}
	if _, err := b.DefinirNota(ctx, tarefa.ID, "daily", "2026-10-02", strings.Repeat("á", MaxNota+1)); !errors.As(err, &ErrInvalido{}) {
		t.Errorf("nota grande demais: %v", err)
	}
	// Nota com cara de senha não é gravada (nem aparece no evento).
	if _, err := b.DefinirNota(ctx, tarefa.ID, "daily", "2026-10-02", "senha: hunter2222"); !errors.As(err, &ErrInvalido{}) {
		t.Errorf("nota com senha aceita: %v", err)
	}
	if notas, _ := b.NotasDoPeriodo(ctx, []int64{tarefa.ID}, "daily", "2026-10-02"); len(notas) != 0 {
		t.Errorf("a nota com senha foi gravada: %+v", notas)
	}
	if _, err := b.DefinirNota(ctx, 999, "daily", "2026-10-02", "x"); !errors.Is(err, ErrNaoEncontrado) {
		t.Errorf("tarefa que não existe: %v", err)
	}
	if err := b.VerificarHistorico(ctx); err != nil {
		t.Error(err)
	}
}

// Um banco da versão 2 (anexos sem tipo, formato e nome, e com o CHECK
// antigo) abre na versão atual (4, com as lousas) com as mesmas linhas e ids.
func TestMigracaoDosAnexos(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "dados")
	b, err := Abrir(dir)
	if err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	tarefa, _ := tarefaComAgente(t, b)
	b.Fechar()

	// Volta a tabela ao formato antigo, com duas linhas, e o banco à versão 2.
	db, err := sql.Open("sqlite", "file:"+filepath.Join(dir, "colmeia.db"))
	if err != nil {
		t.Fatal(err)
	}
	for _, comando := range []string{
		`DROP TABLE anexos`,
		`CREATE TABLE anexos (id INTEGER PRIMARY KEY, perfil_id INTEGER NOT NULL REFERENCES perfis(id) ON DELETE CASCADE, tarefa_id INTEGER,
			sha256 TEXT NOT NULL, largura INTEGER NOT NULL, altura INTEGER NOT NULL, bytes INTEGER NOT NULL,
			origem TEXT NOT NULL CHECK (origem IN ('captura', 'colagem', 'mensagem')), legenda TEXT NOT NULL DEFAULT '',
			criado_em TEXT NOT NULL, removido INTEGER NOT NULL DEFAULT 0)`,
		fmt.Sprintf(`INSERT INTO anexos VALUES (5, 1, %d, '%s', 10, 20, 300, 'captura', '', '2026-09-25T10:00:00Z', 0)`, tarefa.ID, strings.Repeat("a", 64)),
		fmt.Sprintf(`INSERT INTO anexos VALUES (9, 1, %d, '%s', 30, 40, 500, 'mensagem', 'legenda', '2026-09-26T10:00:00Z', 1)`, tarefa.ID, strings.Repeat("b", 64)),
		`PRAGMA user_version = 2`,
	} {
		if _, err := db.Exec(comando); err != nil {
			t.Fatalf("%s: %v", comando, err)
		}
	}
	if _, err := db.Exec(`INSERT INTO anexos VALUES (10, 1, NULL, 'c', 1, 1, 1, 'arquivo', '', 'x', 0)`); err == nil {
		t.Fatal("o banco antigo de teste aceitou a origem nova")
	}
	db.Close()

	b, err = Abrir(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer b.Fechar()
	var versao int
	b.db.QueryRow(`PRAGMA user_version`).Scan(&versao)
	if versao != 5 {
		t.Errorf("versão %d depois de abrir", versao)
	}
	a, err := b.Anexo(ctx, 5)
	if err != nil || a.Largura != 10 || a.Tipo != "imagem" || a.Formato != "png" || a.Origem != "captura" {
		t.Errorf("anexo 5 migrado: %+v, %v", a, err)
	}
	a, err = b.Anexo(ctx, 9)
	if err != nil || !a.Removido || a.Legenda != "legenda" || a.Bytes != 500 {
		t.Errorf("anexo 9 migrado: %+v, %v", a, err)
	}
	video, err := b.CriarAnexo(ctx, NovoAnexo{Perfil: 1, Tarefa: tarefa.ID, Sha256: strings.Repeat("c", 64), Bytes: 1000, Origem: "arquivo", Tipo: "video", Formato: "mp4", Nome: "demo.mp4"})
	if err != nil || video.ID != 10 {
		t.Fatalf("vídeo depois da migração: %+v, %v", video, err)
	}
	lista, _ := b.AnexosDasTarefas(ctx, []int64{tarefa.ID}, "", "")
	if len(lista) != 2 || lista[0].ID != 5 || lista[1].Nome != "demo.mp4" {
		t.Errorf("anexos não removidos da tarefa: %+v", lista)
	}
	if lista, _ := b.AnexosDasTarefas(ctx, []int64{tarefa.ID}, "2026-09-25T12:00:00Z", "2026-09-30T00:00:00Z"); len(lista) != 0 {
		t.Errorf("filtro de período: %+v", lista)
	}
	// O índice voltou.
	var indice int
	b.db.QueryRow(`SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'anexos_por_tarefa'`).Scan(&indice)
	if indice != 1 {
		t.Error("o índice de anexos por tarefa sumiu")
	}
	for _, nome := range []string{"../x.mp4", "pasta/x.mp4", "a\nb"} {
		if _, err := b.CriarAnexo(ctx, NovoAnexo{Perfil: 1, Sha256: strings.Repeat("d", 64), Origem: "arquivo", Tipo: "video", Formato: "mp4", Nome: nome}); err == nil {
			t.Errorf("nome %q aceito", nome)
		}
	}
	if _, err := b.CriarAnexo(ctx, NovoAnexo{Perfil: 1, Sha256: strings.Repeat("d", 64), Origem: "arquivo", Formato: "desktop"}); err == nil {
		t.Error("formato desconhecido aceito")
	}
}
