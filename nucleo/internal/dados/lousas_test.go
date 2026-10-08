package dados

import (
	"context"
	"errors"
	"strings"
	"testing"
)

// lousaDeTeste: um perfil com o workspace Estudos, o projeto loja-web, duas
// tarefas e um segundo perfil (para as referências cruzadas).
type lousaDeTeste struct {
	b                *Banco
	perfil, outro    int64
	workspace        int64
	tarefa, tarefa2  int64
	tarefaDoOutro    int64
	agente           int64
	anexo, video     int64
	anexoDoOutro     int64
	anexoDaTarefa    int64
	anexoDaOutraTrf  int64
	lousaWS, lousaTr int64
}

func f(v float64) *float64 { return &v }

func montarLousas(t *testing.T) lousaDeTeste {
	t.Helper()
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	var d lousaDeTeste
	d.b = b
	p, _ := b.CriarPerfil(ctx, "Pessoal", "")
	o, _ := b.CriarPerfil(ctx, "Outro", "")
	d.perfil, d.outro = p.ID, o.ID
	ws, _ := b.CriarWorkspace(ctx, p.ID, "Estudos")
	wsOutro, _ := b.CriarWorkspace(ctx, o.ID, "W")
	d.workspace = ws.ID
	projeto, _ := b.CriarProjeto(ctx, ws.ID, "loja-web", "/tmp/loja-web", "git", "main")
	projetoOutro, _ := b.CriarProjeto(ctx, wsOutro.ID, "cliente-x", "/tmp/cliente-x", "pasta", "")
	t1, _ := b.CriarTarefa(ctx, projeto.ID, "Nova tela de pedidos", "main", "")
	t2, _ := b.CriarTarefa(ctx, projeto.ID, "Filtro de pedidos", "main", "")
	t3, _ := b.CriarTarefa(ctx, projetoOutro.ID, "Relatório", "", "")
	d.tarefa, d.tarefa2, d.tarefaDoOutro = t1.ID, t2.ID, t3.ID
	a, err := b.CriarAgente(ctx, t1.ID, "claude", "dev", "")
	if err != nil {
		t.Fatal(err)
	}
	d.agente = a.ID
	novo := func(n NovoAnexo) int64 {
		an, err := b.CriarAnexo(ctx, n)
		if err != nil {
			t.Fatal(err)
		}
		return an.ID
	}
	d.anexo = novo(NovoAnexo{Perfil: p.ID, Sha256: "a1", Largura: 800, Altura: 400, Bytes: 10, Origem: "colagem", NaLousa: true})
	d.video = novo(NovoAnexo{Perfil: p.ID, Sha256: "v1", Bytes: 10, Origem: "arquivo", Tipo: "video", Formato: "mp4", NaLousa: true})
	d.anexoDoOutro = novo(NovoAnexo{Perfil: o.ID, Sha256: "a2", Largura: 10, Altura: 10, Bytes: 10, Origem: "colagem", NaLousa: true})
	d.anexoDaTarefa = novo(NovoAnexo{Perfil: p.ID, Tarefa: t1.ID, Sha256: "a3", Largura: 1000, Altura: 500, Bytes: 10, Origem: "captura"})
	d.anexoDaOutraTrf = novo(NovoAnexo{Perfil: p.ID, Tarefa: t2.ID, Sha256: "a4", Largura: 10, Altura: 10, Bytes: 10, Origem: "captura"})
	lw, _, err := b.AbrirLousa(ctx, DonoLousa{WorkspaceID: ws.ID})
	if err != nil {
		t.Fatal(err)
	}
	lt, _, err := b.AbrirLousa(ctx, DonoLousa{TarefaID: t1.ID})
	if err != nil {
		t.Fatal(err)
	}
	d.lousaWS, d.lousaTr = lw.ID, lt.ID
	return d
}

func nota(texto string) *NovoElemento {
	return &NovoElemento{Tipo: "nota", X: f(10), Y: f(20), Largura: f(240), Altura: f(120), Texto: texto}
}

func criar(ref string, e *NovoElemento) Operacao { return Operacao{Op: "criar", Ref: ref, Elemento: e} }

func TestAbrirLousaUmaPorDono(t *testing.T) {
	d := montarLousas(t)
	ctx := context.Background()
	deNovo, elementos, err := d.b.AbrirLousa(ctx, DonoLousa{WorkspaceID: d.workspace})
	if err != nil || deNovo.ID != d.lousaWS || len(elementos) != 0 {
		t.Fatalf("abrir de novo: %+v %v %v", deNovo, elementos, err)
	}
	if deNovo.Dono.WorkspaceID != d.workspace || deNovo.Dono.TarefaID != 0 || deNovo.PerfilID != d.perfil {
		t.Errorf("dono da lousa: %+v", deNovo)
	}
	if _, _, err := d.b.AbrirLousa(ctx, DonoLousa{WorkspaceID: d.workspace, TarefaID: d.tarefa}); err == nil {
		t.Error("uma lousa com dois donos deveria ser recusada")
	}
	if _, _, err := d.b.AbrirLousa(ctx, DonoLousa{TarefaID: 999}); !errors.Is(err, ErrNaoEncontrado) {
		t.Errorf("tarefa inexistente: %v", err)
	}
	// O CHECK do banco também segura um dono só.
	if _, err := d.b.db.Exec(`INSERT INTO lousas (perfil_id, workspace_id, tarefa_id, criada_em) VALUES (?, ?, ?, 'x')`, d.perfil, nil, nil); err == nil {
		t.Error("lousa sem dono passou pelo banco")
	}
	// Abrir não gera evento.
	eventos, _ := d.b.ListarEventos(ctx, FiltroEventos{Perfil: d.perfil, Tipos: []string{"lousa.agente"}})
	if len(eventos) != 0 {
		t.Errorf("abrir a lousa gerou eventos: %v", eventos)
	}
}

func TestLimitesETextos(t *testing.T) {
	d := montarLousas(t)
	ctx := context.Background()
	casos := map[string]*NovoElemento{
		"tipo":       {Tipo: "forma", X: f(0), Y: f(0), Largura: f(100), Altura: f(100)},
		"cor":        {Tipo: "nota", X: f(0), Y: f(0), Largura: f(100), Altura: f(100), Cor: "preto"},
		"longe":      {Tipo: "nota", X: f(2e6), Y: f(0), Largura: f(100), Altura: f(100)},
		"pequeno":    {Tipo: "nota", X: f(0), Y: f(0), Largura: f(10), Altura: f(100)},
		"grande":     {Tipo: "nota", X: f(0), Y: f(0), Largura: f(100), Altura: f(7000)},
		"sem x":      {Tipo: "nota", Y: f(0), Largura: f(100), Altura: f(100)},
		"controle":   {Tipo: "nota", X: f(0), Y: f(0), Largura: f(100), Altura: f(100), Texto: "a\x07b"},
		"título":     {Tipo: "nota", X: f(0), Y: f(0), Largura: f(100), Altura: f(100), Titulo: "linha\nquebrada"},
		"longo":      {Tipo: "nota", X: f(0), Y: f(0), Largura: f(100), Altura: f(100), Texto: strings.Repeat("a", MaxTextoLousa+1)},
		"segredo":    {Tipo: "codigo", X: f(0), Y: f(0), Largura: f(100), Altura: f(100), Texto: "export API_KEY=abcdef123456"},
		"anexo sobr": {Tipo: "nota", X: f(0), Y: f(0), Largura: f(100), Altura: f(100), AnexoID: d.anexo},
	}
	for nome, e := range casos {
		_, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{criar("", e)})
		var invalido ErrInvalido
		if !errors.As(err, &invalido) {
			t.Errorf("%s: esperado ErrInvalido, veio %v", nome, err)
			continue
		}
		// A mensagem nunca repete o texto do usuário.
		if strings.Contains(invalido.Motivo, "abcdef") || strings.Contains(invalido.Motivo, "\x07") {
			t.Errorf("%s: a mensagem repete o texto: %q", nome, invalido.Motivo)
		}
	}
	// Tabulação e quebra de linha passam; \r\n vira \n.
	r, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{criar("a", nota("# Título\r\n\t- item"))})
	if err != nil || r.Elementos[0].Texto != "# Título\n\t- item" || r.Elementos[0].Cor != "amarelo" || r.Elementos[0].Versao != 1 {
		t.Fatalf("nota válida: %+v %v", r, err)
	}
	if r.Refs["a"] != r.Elementos[0].ID {
		t.Errorf("refs: %v", r.Refs)
	}
	// Lote vazio ou grande demais.
	if _, err := d.b.GravarLousa(ctx, d.lousaWS, nil); err == nil {
		t.Error("lote vazio passou")
	}
	muitos := make([]Operacao, MaxOperacoesLousa+1)
	for i := range muitos {
		muitos[i] = criar("", nota("x"))
	}
	if _, err := d.b.GravarLousa(ctx, d.lousaWS, muitos); err == nil {
		t.Error("lote com mais de 500 operações passou")
	}
}

func TestLimiteDeElementos(t *testing.T) {
	d := montarLousas(t)
	ctx := context.Background()
	lote := make([]Operacao, MaxOperacoesLousa)
	for i := range lote {
		lote[i] = criar("", nota(""))
	}
	for range MaxElementosLousa / MaxOperacoesLousa {
		if _, err := d.b.GravarLousa(ctx, d.lousaWS, lote); err != nil {
			t.Fatal(err)
		}
	}
	_, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{criar("", nota(""))})
	var invalido ErrInvalido
	if !errors.As(err, &invalido) || !strings.Contains(invalido.Motivo, "2000") {
		t.Fatalf("o 2001º item: %v", err)
	}
	_, elementos, _ := d.b.Lousa(ctx, d.lousaWS)
	if len(elementos) != MaxElementosLousa {
		t.Errorf("ficaram %d itens", len(elementos))
	}
}

func TestReferenciasCruzadas(t *testing.T) {
	d := montarLousas(t)
	ctx := context.Background()
	img := func(anexo int64) *NovoElemento {
		return &NovoElemento{Tipo: "imagem", X: f(0), Y: f(0), Largura: f(200), Altura: f(100), AnexoID: anexo}
	}
	if _, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{criar("", img(d.anexoDoOutro))}); err == nil {
		t.Error("anexo de outro perfil passou")
	}
	if _, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{criar("", img(d.video))}); err == nil {
		t.Error("vídeo como imagem passou")
	}
	r, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{criar("", img(d.anexo))})
	if err != nil || r.Elementos[0].Anexo == nil || r.Elementos[0].Anexo.Largura != 800 {
		t.Fatalf("imagem do perfil: %+v %v", r, err)
	}
	// Anexo removido não entra.
	d.b.RemoverAnexo(ctx, d.anexoDaOutraTrf)
	if _, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{criar("", img(d.anexoDaOutraTrf))}); err == nil {
		t.Error("anexo removido passou")
	}
	cartao := func(tarefa int64) *NovoElemento {
		return &NovoElemento{Tipo: "tarefa", X: f(0), Y: f(0), Largura: f(260), Altura: f(96), TarefaRef: tarefa}
	}
	if _, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{criar("", cartao(d.tarefaDoOutro))}); err == nil {
		t.Error("tarefa de outro perfil passou")
	}
	if _, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{criar("", cartao(d.tarefa2))}); err != nil {
		t.Errorf("cartão de tarefa do perfil: %v", err)
	}
	// Ligação para outra lousa, para uma ligação e para si mesma.
	naTarefa, _ := d.b.GravarLousa(ctx, d.lousaTr, []Operacao{criar("", nota("na tarefa"))})
	outraLousa := naTarefa.Elementos[0].ID
	base, _ := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{criar("a", nota("a")), criar("b", nota("b")),
		criar("l", &NovoElemento{Tipo: "ligacao", De: &RefElemento{Ref: "a"}, Para: &RefElemento{Ref: "b"}, Texto: "depois"})})
	a, ligacao := base.Refs["a"], base.Refs["l"]
	for nome, e := range map[string]*NovoElemento{
		"outra lousa": {Tipo: "ligacao", De: &RefElemento{ID: a}, Para: &RefElemento{ID: outraLousa}},
		"ligação":     {Tipo: "ligacao", De: &RefElemento{ID: a}, Para: &RefElemento{ID: ligacao}},
		"a mesma":     {Tipo: "ligacao", De: &RefElemento{ID: a}, Para: &RefElemento{ID: a}},
		"sem para":    {Tipo: "ligacao", De: &RefElemento{ID: a}},
		"ref depois":  {Tipo: "ligacao", De: &RefElemento{ID: a}, Para: &RefElemento{Ref: "z"}},
	} {
		if _, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{criar("", e)}); err == nil {
			t.Errorf("ligação inválida (%s) passou", nome)
		}
	}
}

func TestLoteAtomicoEConflito(t *testing.T) {
	d := montarLousas(t)
	ctx := context.Background()
	r, _ := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{criar("a", nota("a")), criar("b", nota("b"))})
	a, b := r.Elementos[0], r.Elementos[1]
	if b.Z <= a.Z {
		t.Error("o item criado depois deveria ficar por cima")
	}
	// Alterar com a versão certa sobe a versão.
	novoX, texto := 300.0, "a editada"
	r, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{{Op: "alterar", ID: a.ID, Versao: 1, Campos: &CamposElemento{X: &novoX, Texto: &texto}}})
	if err != nil || r.Elementos[0].Versao != 2 || r.Elementos[0].X != 300 || r.Elementos[0].Texto != "a editada" || r.Elementos[0].Y != a.Y {
		t.Fatalf("alterar: %+v %v", r, err)
	}
	// Um lote com uma versão velha não grava nada, nem a parte certa.
	outroX := 999.0
	_, err = d.b.GravarLousa(ctx, d.lousaWS, []Operacao{
		{Op: "alterar", ID: b.ID, Versao: 1, Campos: &CamposElemento{X: &outroX}},
		{Op: "alterar", ID: a.ID, Versao: 1, Campos: &CamposElemento{X: &outroX}},
		{Op: "remover", ID: 98765, Versao: 1},
	})
	var mudou ErrLousaMudou
	if !errors.As(err, &mudou) {
		t.Fatalf("esperado ErrLousaMudou, veio %v", err)
	}
	if len(mudou.Elementos) != 1 || mudou.Elementos[0].ID != a.ID || mudou.Elementos[0].Versao != 2 || len(mudou.Removidos) != 1 || mudou.Removidos[0] != 98765 {
		t.Errorf("conflito: %+v", mudou)
	}
	_, todos, _ := d.b.Lousa(ctx, d.lousaWS)
	for _, e := range todos {
		if e.X == 999 {
			t.Error("o lote com conflito gravou uma parte")
		}
	}
	// Trocar o tipo: só entre os de texto.
	codigo := "codigo"
	if _, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{{Op: "alterar", ID: b.ID, Versao: 1, Campos: &CamposElemento{Tipo: &codigo}}}); err != nil {
		t.Errorf("nota para código: %v", err)
	}
	ligacao := "ligacao"
	if _, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{{Op: "alterar", ID: b.ID, Versao: 2, Campos: &CamposElemento{Tipo: &ligacao}}}); err == nil {
		t.Error("código virou ligação")
	}
	// O mesmo item duas vezes no lote.
	if _, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{{Op: "remover", ID: b.ID, Versao: 2}, {Op: "remover", ID: b.ID, Versao: 2}}); err == nil {
		t.Error("o mesmo item duas vezes passou")
	}
	// Um item de outra lousa conta como removido daqui.
	naTarefa, _ := d.b.GravarLousa(ctx, d.lousaTr, []Operacao{criar("", nota("x"))})
	_, err = d.b.GravarLousa(ctx, d.lousaWS, []Operacao{{Op: "remover", ID: naTarefa.Elementos[0].ID, Versao: 1}})
	if !errors.As(err, &mudou) || len(mudou.Removidos) != 1 {
		t.Errorf("item de outra lousa: %v", err)
	}
}

func TestCascatas(t *testing.T) {
	d := montarLousas(t)
	ctx := context.Background()
	r, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{
		criar("a", nota("a")), criar("b", nota("b")),
		criar("l", &NovoElemento{Tipo: "ligacao", De: &RefElemento{Ref: "a"}, Para: &RefElemento{Ref: "b"}}),
		criar("t", &NovoElemento{Tipo: "tarefa", X: f(0), Y: f(0), Largura: f(260), Altura: f(96), TarefaRef: d.tarefa2}),
	})
	if err != nil {
		t.Fatal(err)
	}
	if r.Elementos[2].De != r.Refs["a"] || r.Elementos[2].Para != r.Refs["b"] {
		t.Errorf("ligação pelos refs: %+v", r.Elementos[2])
	}
	// Remover um item leva as ligações dele, e a resposta diz isso.
	r2, err := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{{Op: "remover", ID: r.Refs["a"], Versao: 1}})
	if err != nil || len(r2.Removidos) != 2 {
		t.Fatalf("remover com ligação: %+v %v", r2, err)
	}
	// Tarefa apagada: o cartão fica sem a tarefa ("tarefa removida").
	if err := d.b.RemoverTarefa(ctx, d.tarefa2); err != nil {
		t.Fatal(err)
	}
	cartao, _ := d.b.ElementosPorID(ctx, []int64{r.Refs["t"]})
	if len(cartao) != 1 || cartao[0].TarefaRef != 0 {
		t.Errorf("cartão da tarefa removida: %+v", cartao)
	}
	// A tarefa apagada leva a lousa dela.
	d.b.GravarLousa(ctx, d.lousaTr, []Operacao{criar("", nota("x"))})
	if err := d.b.RemoverTarefa(ctx, d.tarefa); err != nil {
		t.Fatal(err)
	}
	if _, _, err := d.b.Lousa(ctx, d.lousaTr); !errors.Is(err, ErrNaoEncontrado) {
		t.Errorf("a lousa da tarefa removida ficou: %v", err)
	}
	var sobrou int
	d.b.db.QueryRow(`SELECT COUNT(*) FROM lousa_elementos WHERE lousa_id = ?`, d.lousaTr).Scan(&sobrou)
	if sobrou != 0 {
		t.Errorf("sobraram %d itens da lousa removida", sobrou)
	}
}

func TestTextoDaLousaFicaForaDosEventos(t *testing.T) {
	d := montarLousas(t)
	ctx := context.Background()
	const marcador = "MARCADOR-LOUSA-7f3a"
	r, _ := d.b.GravarLousa(ctx, d.lousaWS, []Operacao{criar("", nota(marcador))})
	novo := marcador + " editado"
	d.b.GravarLousa(ctx, d.lousaWS, []Operacao{{Op: "alterar", ID: r.Elementos[0].ID, Versao: 1, Campos: &CamposElemento{Texto: &novo}}})
	_, err := d.b.AcrescentarDoAgente(ctx, d.tarefa, d.agente, []NovoDoAgente{{NovoElemento: NovoElemento{Tipo: "nota", Texto: marcador, Titulo: marcador}}})
	if err != nil {
		t.Fatal(err)
	}
	var n int
	d.b.db.QueryRow(`SELECT COUNT(*) FROM eventos WHERE dados LIKE ?`, "%"+marcador+"%").Scan(&n)
	if n != 0 {
		t.Errorf("o texto da lousa foi parar em %d eventos", n)
	}
	// Só o agente entra na corrente.
	eventos, _ := d.b.ListarEventos(ctx, FiltroEventos{Perfil: d.perfil, Tipos: []string{"lousa.agente"}})
	if len(eventos) != 1 || eventos[0].Escopo.Agente != d.agente || eventos[0].Escopo.Tarefa != d.tarefa {
		t.Fatalf("eventos do agente: %+v", eventos)
	}
	if !strings.Contains(string(eventos[0].Dados), `"quantidade":1`) || !strings.Contains(string(eventos[0].Dados), `"nota":1`) {
		t.Errorf("conteúdo do evento: %s", eventos[0].Dados)
	}
	if err := d.b.VerificarHistorico(ctx); err != nil {
		t.Error(err)
	}
}

func TestAgenteSoAcrescentaNaLousaDaTarefa(t *testing.T) {
	d := montarLousas(t)
	ctx := context.Background()
	// O que já existe na lousa da tarefa não é coberto.
	existente, _ := d.b.GravarLousa(ctx, d.lousaTr, []Operacao{criar("", &NovoElemento{Tipo: "nota", X: f(0), Y: f(0), Largura: f(400), Altura: f(300)})})
	novos := []NovoDoAgente{
		{Ref: "tela", NovoElemento: NovoElemento{Tipo: "nota", Texto: "# Tela de pedidos\n- lista\n- filtro"}},
		{Ref: "api", NovoElemento: NovoElemento{Tipo: "codigo", Titulo: "fluxo", Texto: "tela -> api -> banco\n  └─ cache"}},
		{NovoElemento: NovoElemento{Tipo: "ligacao", De: &RefElemento{Ref: "tela"}, Para: &RefElemento{Ref: "api"}, Texto: "chama"}},
		{NovoElemento: NovoElemento{Tipo: "imagem", AnexoID: d.anexoDaTarefa}},
	}
	r, err := d.b.AcrescentarDoAgente(ctx, d.tarefa, d.agente, novos)
	if err != nil {
		t.Fatal(err)
	}
	if len(r.Elementos) != 4 {
		t.Fatalf("acrescentados: %+v", r.Elementos)
	}
	ocupado := retangulo{0, 0, 400, 300}
	var colocados []retangulo
	for _, e := range r.Elementos {
		if e.Autor != "agente" || e.AgenteID != d.agente || e.LousaID != d.lousaTr {
			t.Errorf("autor do item: %+v", e)
		}
		if e.Tipo == "ligacao" {
			continue
		}
		ret := retangulo{e.X, e.Y, e.Largura, e.Altura}
		if ret.cruza(ocupado) {
			t.Errorf("%s cobre o que já existia: %+v", e.Tipo, ret)
		}
		for _, outro := range colocados {
			if ret.cruza(outro) {
				t.Errorf("%s cobre outro item novo", e.Tipo)
			}
		}
		colocados = append(colocados, ret)
		if e.Tipo == "imagem" && (e.Largura != 480 || e.Altura != 240) {
			t.Errorf("imagem com a proporção do anexo: %vx%v", e.Largura, e.Altura)
		}
	}
	// Só os tipos dele, só anexos da tarefa dele, só acrescentar.
	recusados := map[string]NovoDoAgente{
		"vídeo":         {NovoElemento: NovoElemento{Tipo: "video", AnexoID: d.video}},
		"cartão":        {NovoElemento: NovoElemento{Tipo: "tarefa", TarefaRef: d.tarefa2}},
		"anexo do perf": {NovoElemento: NovoElemento{Tipo: "imagem", AnexoID: d.anexo}},
		"outra tarefa":  {NovoElemento: NovoElemento{Tipo: "imagem", AnexoID: d.anexoDaOutraTrf}},
	}
	for nome, n := range recusados {
		if _, err := d.b.AcrescentarDoAgente(ctx, d.tarefa, d.agente, []NovoDoAgente{n}); err == nil {
			t.Errorf("o agente acrescentou %s", nome)
		}
	}
	if _, err := d.b.AcrescentarDoAgente(ctx, d.tarefa2, d.agente, []NovoDoAgente{{NovoElemento: NovoElemento{Tipo: "nota", Texto: "x"}}}); err == nil {
		t.Error("o agente escreveu na lousa de outra tarefa")
	}
	muitos := make([]NovoDoAgente, MaxDoAgente+1)
	for i := range muitos {
		muitos[i] = NovoDoAgente{NovoElemento: NovoElemento{Tipo: "nota", Texto: "x"}}
	}
	if _, err := d.b.AcrescentarDoAgente(ctx, d.tarefa, d.agente, muitos); err == nil {
		t.Error("o agente acrescentou mais de 50 de uma vez")
	}
	if _, err := aplicarLoteComo(t, d, Operacao{Op: "remover", ID: existente.Elementos[0].ID, Versao: 1}); err == nil {
		t.Error("o agente removeu um item")
	}
}

// aplicarLoteComo aplica uma operação como agente (o caminho que a API não expõe).
func aplicarLoteComo(t *testing.T, d lousaDeTeste, op Operacao) (ResultadoLousa, error) {
	t.Helper()
	var r ResultadoLousa
	err := d.b.emTransacao(context.Background(), func(tx *transacao) error {
		l, err := lerLousa(context.Background(), tx, d.lousaTr)
		if err != nil {
			return err
		}
		r, err = aplicarLote(context.Background(), tx, l, []Operacao{op}, autorLousa{agente: d.agente, soDaTarefa: d.tarefa})
		return err
	})
	return r, err
}

func TestPosicaoAutomaticaEmColunas(t *testing.T) {
	existentes := []retangulo{{-100, 50, 300, 200}}
	var tamanhos []retangulo
	for range 12 {
		tamanhos = append(tamanhos, retangulo{largura: 260, altura: 300})
	}
	pos := posicaoAutomatica(existentes, tamanhos)
	if pos[0].x != 200+2*vaoGrade || pos[0].y != 50 {
		t.Errorf("primeiro item: %+v", pos[0])
	}
	colunas := map[float64]bool{}
	for i, p := range pos {
		colunas[p.x] = true
		if p.y+p.altura > 50+alturaDaColuna+300 {
			t.Errorf("item %d passou da altura da coluna: %+v", i, p)
		}
		for _, e := range existentes {
			if p.cruza(e) {
				t.Errorf("item %d cobre o existente", i)
			}
		}
	}
	if len(colunas) < 3 {
		t.Errorf("12 itens altos deveriam ocupar várias colunas: %v", colunas)
	}
	if v := posicaoAutomatica(nil, []retangulo{{largura: 10, altura: 10}}); v[0].x != 0 || v[0].y != 0 {
		t.Errorf("lousa vazia começa na origem: %+v", v)
	}
}

func TestPosicaoDadaOcupadaAndaAteOLivre(t *testing.T) {
	ocupados := []retangulo{{0, 0, 260, 150}, {300, 0, 260, 150}}
	// Livre: fica onde está.
	if r := lugarLivre(retangulo{0, 400, 100, 100}, ocupados, folgaDoAgente); r.x != 0 || r.y != 400 {
		t.Errorf("posição livre mudou: %+v", r)
	}
	// Em cima do primeiro: desce (mais perto que passar dos dois à direita).
	r := lugarLivre(retangulo{-220, 36, 260, 140}, ocupados, folgaDoAgente)
	for _, o := range ocupados {
		if (retangulo{r.x - folgaDoAgente, r.y - folgaDoAgente, r.largura + 2*folgaDoAgente, r.altura + 2*folgaDoAgente}).cruza(o) {
			t.Errorf("ainda cobre ou encosta em %+v: %+v", o, r)
		}
	}
	if r.y != 150+folgaDoAgente || r.x != -220 {
		t.Errorf("esperava descer para y=174: %+v", r)
	}
}

func TestAgenteNaoCobreONemComPosicaoDada(t *testing.T) {
	d := montarLousas(t)
	ctx := context.Background()
	x, y := 0.0, 0.0
	primeiro := []NovoDoAgente{{NovoElemento: NovoElemento{Tipo: "nota", Texto: "a", X: &x, Y: &y}}}
	if _, err := d.b.AcrescentarDoAgente(ctx, d.tarefa, d.agente, primeiro); err != nil {
		t.Fatal(err)
	}
	// O agente manda de novo para o mesmo ponto, e mais dois sem posição.
	x2, y2 := 10.0, 10.0
	segundo := []NovoDoAgente{
		{NovoElemento: NovoElemento{Tipo: "nota", Texto: "b", X: &x2, Y: &y2}},
		{Ref: "c", NovoElemento: NovoElemento{Tipo: "nota", Texto: "c"}},
		{Ref: "d", NovoElemento: NovoElemento{Tipo: "codigo", Texto: "d"}},
	}
	if _, err := d.b.AcrescentarDoAgente(ctx, d.tarefa, d.agente, segundo); err != nil {
		t.Fatal(err)
	}
	_, elementos, err := d.b.Lousa(ctx, d.lousaTr)
	if err != nil {
		t.Fatal(err)
	}
	var itens []retangulo
	for _, e := range elementos {
		if e.Tipo != "ligacao" {
			itens = append(itens, retangulo{e.X, e.Y, e.Largura, e.Altura})
		}
	}
	for i := range itens {
		for j := range i {
			if itens[i].cruza(itens[j]) {
				t.Errorf("itens %d e %d se cruzam: %+v %+v", i, j, itens[i], itens[j])
			}
		}
	}
}

func TestTamanhoEstimadoPeloTexto(t *testing.T) {
	l, a := tamanhoEstimado("codigo", "", strings.Repeat("x", 80)+"\nb\nc")
	if l != 80*7.5+32 || a != 3*18+24 {
		t.Errorf("código: %vx%v", l, a)
	}
	_, curta := tamanhoEstimado("nota", "", "uma linha")
	_, longa := tamanhoEstimado("nota", "", strings.Repeat("palavra ", 200))
	if longa <= curta*3 {
		t.Errorf("nota longa (%v) deveria ser bem mais alta que a curta (%v)", longa, curta)
	}
	lt, _ := tamanhoEstimado("nota", "", "| coluna um | coluna dois | coluna três | coluna quatro |")
	if lt <= 260 {
		t.Errorf("nota com tabela deveria alargar: %v", lt)
	}
}

func TestMigracaoDeUmBancoV3(t *testing.T) {
	b, dir := bancoDeTeste(t)
	ctx := context.Background()
	p, _ := b.CriarPerfil(ctx, "Pessoal", "")
	b.CriarAnexo(ctx, NovoAnexo{Perfil: p.ID, Sha256: "x", Largura: 1, Altura: 1, Bytes: 1, Origem: "colagem"})
	// Volta o banco à versão 3: sem lousas e sem anexos.na_lousa.
	for _, comando := range []string{`DROP TABLE lousa_elementos`, `DROP TABLE lousas`, `ALTER TABLE anexos DROP COLUMN na_lousa`, `PRAGMA user_version = 3`} {
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
	if versao != 8 {
		t.Errorf("versão do banco: %d", versao)
	}
	a, err := b2.Anexo(ctx, 1)
	if err != nil || a.NaLousa {
		t.Errorf("anexo antigo: %+v %v", a, err)
	}
	ws, _ := b2.CriarWorkspace(ctx, p.ID, "Estudos")
	if _, _, err := b2.AbrirLousa(ctx, DonoLousa{WorkspaceID: ws.ID}); err != nil {
		t.Errorf("lousa depois da migração: %v", err)
	}
}

func TestAnexoDaLousaForaDosSlides(t *testing.T) {
	d := montarLousas(t)
	ctx := context.Background()
	lista, _ := d.b.AnexosDasTarefas(ctx, []int64{d.tarefa}, "", "")
	if len(lista) != 1 || lista[0].ID != d.anexoDaTarefa {
		t.Errorf("anexos da tarefa: %+v", lista)
	}
	if _, err := d.b.CriarAnexo(ctx, NovoAnexo{Perfil: d.perfil, Tarefa: d.tarefa, Sha256: "z", Largura: 1, Altura: 1, Bytes: 1, Origem: "colagem", NaLousa: true}); err == nil {
		t.Error("anexo da lousa com tarefa passou")
	}
	resumo, _ := d.b.LousasDasTarefas(ctx, []int64{d.tarefa, d.tarefa2})
	if len(resumo) != 0 {
		t.Errorf("lousa vazia no resumo: %v", resumo)
	}
	d.b.GravarLousa(ctx, d.lousaTr, []Operacao{criar("", nota("x")), criar("", nota("y"))})
	resumo, _ = d.b.LousasDasTarefas(ctx, []int64{d.tarefa, d.tarefa2})
	if resumo[d.tarefa] != (ResumoLousa{ID: d.lousaTr, Elementos: 2}) {
		t.Errorf("resumo da lousa: %v", resumo)
	}
}
