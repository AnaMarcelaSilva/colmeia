package dados

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestConexoesDeBanco(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	p, _ := b.CriarPerfil(ctx, "Pessoal", "")
	c, err := b.CriarConexao(ctx, p.ID, CamposConexao{Nome: " loja-web-dev ", Tipo: "postgres", Host: "localhost", Usuario: "app", Banco: "loja"}, "chaveiro")
	if err != nil {
		t.Fatal(err)
	}
	if c.Nome != "loja-web-dev" || c.Porta != 5432 || c.SSL != "preferir" || !strings.HasPrefix(c.ChaveSegredo, "conexao:") || c.Senha != "chaveiro" {
		t.Fatalf("criada: %+v", c)
	}
	if _, err := b.CriarConexao(ctx, p.ID, CamposConexao{Nome: "loja-web-dev", Tipo: "mysql", Host: "x"}, "nenhuma"); !errors.Is(err, ErrJaExiste) {
		t.Fatalf("nome repetido: %v", err)
	}
	c2, err := b.AtualizarConexao(ctx, c.ID, CamposConexao{Nome: "loja", Tipo: "postgres", Host: "127.0.0.1", Porta: 54321, Escrita: true, Pasta: "Trabalho"})
	if err != nil || c2.Host != "127.0.0.1" || !c2.Escrita || c2.ChaveSegredo != c.ChaveSegredo || c2.Senha != "chaveiro" {
		t.Fatalf("atualizada: %+v %v", c2, err)
	}
	if _, err := b.AtualizarConexao(ctx, c.ID, CamposConexao{Nome: "loja", Tipo: "mysql", Host: "x"}); err == nil {
		t.Fatal("o tipo não muda")
	}
	if err := b.DefinirOndeSenha(ctx, c.ID, "memoria"); err != nil {
		t.Fatal(err)
	}
	lista, _ := b.ListarConexoes(ctx, p.ID)
	if len(lista) != 1 || lista[0].Senha != "memoria" || lista[0].Pasta != "Trabalho" {
		t.Fatalf("lista: %+v", lista)
	}
	b.GuardarConsulta(ctx, ConsultaBanco{ConexaoID: c.ID, SQL: "SELECT 1", Origem: "voce"})
	if guardou, _ := b.GuardarConsulta(ctx, ConsultaBanco{ConexaoID: c.ID, SQL: "CREATE USER x PASSWORD 'segredo123!'", Origem: "voce"}); guardou {
		t.Fatal("SQL com senha não fica no histórico")
	}
	removida, err := b.RemoverConexao(ctx, c.ID)
	if err != nil || removida.ChaveSegredo != c.ChaveSegredo {
		t.Fatalf("removida: %+v %v", removida, err)
	}
	if h, _ := b.ListarConsultas(ctx, c.ID, 0); len(h) != 0 {
		t.Fatalf("o histórico sai junto: %+v", h)
	}
	eventos, _ := b.ListarEventos(ctx, FiltroEventos{Perfil: p.ID, Tipos: []string{"banco.conexao"}})
	if len(eventos) != 3 {
		t.Fatalf("eventos: %d", len(eventos))
	}
	for _, e := range eventos {
		if strings.Contains(string(e.Dados), "conexao:") {
			t.Fatalf("a conta do chaveiro não vai para o evento: %s", e.Dados)
		}
	}
	if err := b.VerificarHistorico(ctx); err != nil {
		t.Fatal(err)
	}
}

func TestValidarConexao(t *testing.T) {
	dir := t.TempDir()
	arquivo := filepath.Join(dir, "loja.db")
	os.WriteFile(arquivo, nil, 0o600)
	invalidos := []CamposConexao{
		{Nome: "", Tipo: "postgres", Host: "x"},
		{Nome: "a", Tipo: "oracle", Host: "x"},
		{Nome: "a", Tipo: "postgres"},
		{Nome: "a", Tipo: "postgres", Host: "x y"},
		{Nome: "a", Tipo: "postgres", Host: "x/y"},
		{Nome: "a", Tipo: "postgres", Host: "a,b"},
		{Nome: "a", Tipo: "postgres", Host: "x", Porta: 70000},
		{Nome: "a", Tipo: "postgres", Host: "x", Usuario: "a\nb"},
		{Nome: "a", Tipo: "postgres", Host: "x", SSL: "talvez"},
		{Nome: "a", Tipo: "postgres", Host: "x", SSL: "verificar", SSLCA: "relativo.pem"},
		{Nome: "a", Tipo: "sqlite", Arquivo: "relativo.db"},
		{Nome: "a", Tipo: "sqlite", Arquivo: filepath.Join(dir, "nao-existe.db")},
		{Nome: "a", Tipo: "sqlite", Arquivo: dir},
		{Nome: strings.Repeat("a", 81), Tipo: "postgres", Host: "x"},
	}
	for _, c := range invalidos {
		if _, err := ValidarConexao(c); err == nil {
			t.Errorf("deveria recusar %+v", c)
		}
	}
	c, err := ValidarConexao(CamposConexao{Nome: "loja", Tipo: "sqlite", Arquivo: arquivo, Host: "ignorado", Porta: 5})
	if err != nil || c.Host != "" || c.Porta != 0 || c.SSL != "desligado" {
		t.Fatalf("sqlite: %+v %v", c, err)
	}
	if c, _ := ValidarConexao(CamposConexao{Nome: "a", Tipo: "sqlserver", Host: "x"}); c.Porta != 1433 {
		t.Fatalf("porta padrão: %d", c.Porta)
	}
}

func TestMigracaoDeUmBancoV4(t *testing.T) {
	b, dir := bancoDeTeste(t)
	ctx := context.Background()
	p, _ := b.CriarPerfil(ctx, "Pessoal", "")
	for _, comando := range []string{`DROP TABLE consultas_banco`, `DROP TABLE conexoes_banco`, `PRAGMA user_version = 4`} {
		if _, err := b.db.Exec(comando); err != nil {
			t.Fatal(comando, err)
		}
	}
	b.Fechar()
	b2, err := Abrir(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer b2.Fechar()
	var versao int
	b2.db.QueryRow(`PRAGMA user_version`).Scan(&versao)
	if versao != 7 {
		t.Errorf("versão: %d", versao)
	}
	if _, err := b2.CriarConexao(ctx, p.ID, CamposConexao{Nome: "loja", Tipo: "mysql", Host: "localhost"}, "nenhuma"); err != nil {
		t.Fatalf("conexão depois da migração: %v", err)
	}
}
