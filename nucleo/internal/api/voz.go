package api

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"log"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"unicode"

	"golang.org/x/text/unicode/norm"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/claudecode"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/voz"
)

// A voz: o núcleo baixa e confere os modelos e guarda as opções no perfil.
// Ouvir, transcrever e falar ficam no colmeia-voz, que a tela (ou a abelha
// fixa) abre; nada de áudio passa por aqui. São dois pacotes de modelo: o
// Whisper (o ditado e os comandos falados) e a voz da abelha (Kokoro).

// O que liga num perfil quando o download que ele pediu termina.
const (
	ligarDitado   = "ditado"
	ligarComandos = "comandos"
	ligarAbelha   = "abelha"
)

// quemBaixa são os perfis que pediram o download em andamento e o que cada
// um liga quando o modelo fica pronto; eles recebem o progresso.
type quemBaixa struct {
	mu     sync.Mutex
	perfis map[int64]string
}

func (q *quemBaixa) juntar(perfil int64, ligar string) {
	q.mu.Lock()
	defer q.mu.Unlock()
	if q.perfis == nil {
		q.perfis = map[int64]string{}
	}
	q.perfis[perfil] = ligar
}

// lista devolve os perfis; com esvaziar, o download acabou e a lista zera.
func (q *quemBaixa) lista(esvaziar bool) map[int64]string {
	q.mu.Lock()
	defer q.mu.Unlock()
	copia := make(map[int64]string, len(q.perfis))
	for id, l := range q.perfis {
		copia[id] = l
	}
	if esvaziar {
		q.perfis = nil
	}
	return copia
}

// pacoteVoz é um modelo para baixar: o gerente e quem pediu.
type pacoteVoz struct {
	nome  string // vai no evento voz.modelo: "whisper" ou "kokoro"
	g     *voz.Gerente
	quem  quemBaixa
	ligas []string // o que este pacote pode ligar
}

func (s *Servidor) iniciarVoz(mux *http.ServeMux) {
	if s.Voz == nil {
		s.Voz = voz.Novo(s.DirDados, voz.Base)
	}
	if s.Fala == nil {
		s.Fala = voz.Novo(s.DirDados, voz.Kokoro...)
	}
	s.whisper = &pacoteVoz{nome: "whisper", g: s.Voz, ligas: []string{ligarDitado, ligarComandos}}
	s.kokoro = &pacoteVoz{nome: "kokoro", g: s.Fala, ligas: []string{ligarAbelha}}
	for _, p := range []*pacoteVoz{s.whisper, s.kokoro} {
		p.g.AoMudar = func(e voz.Estado) {
			for perfil := range p.quem.lista(false) {
				s.Avisos.Publicar(perfil, map[string]any{"tipo": "voz.modelo", "pacote": p.nome, "estado": e.Estado, "baixados": e.Baixados, "tamanho": e.Tamanho, "erro": e.Erro})
			}
		}
	}
	mux.HandleFunc("GET /v1/voz", s.estadoVoz)
	mux.HandleFunc("POST /v1/perfis/{id}/voz/baixar", s.baixarVoz)
	mux.HandleFunc("POST /v1/voz/cancelar", s.cancelarVoz)
	mux.HandleFunc("DELETE /v1/voz/modelo", s.removerVoz)
	mux.HandleFunc("GET /v1/voz/fala", s.estadoFala)
	mux.HandleFunc("POST /v1/perfis/{id}/voz/fala/baixar", s.baixarFala)
	mux.HandleFunc("POST /v1/voz/fala/cancelar", s.cancelarFala)
	mux.HandleFunc("DELETE /v1/voz/fala", s.removerFala)
	mux.HandleFunc("GET /v1/perfis/{id}/voz/nome", s.nomeOuvido)
	mux.HandleFunc("DELETE /v1/perfis/{id}/voz/nome", s.esquecerNome)
	s.rotasComandoVoz(mux)
}

func (s *Servidor) estadoVoz(w http.ResponseWriter, _ *http.Request) {
	responderJSON(w, s.Voz.Estado())
}

// estadoFala: no sistema sem pacote conferido, "indisponivel".
func (s *Servidor) estadoFala(w http.ResponseWriter, _ *http.Request) {
	if !voz.KokoroDisponivel() {
		responderJSON(w, voz.Estado{Estado: voz.Indisponivel})
		return
	}
	responderJSON(w, s.Fala.Estado())
}

// baixarVoz começa o download do Whisper (o tamanho já foi mostrado na
// tela). O corpo diz o que ligar no fim: o ditado (padrão) ou os comandos.
func (s *Servidor) baixarVoz(w http.ResponseWriter, r *http.Request) {
	var pedido struct {
		Ligar string `json:"ligar"`
	}
	if r.ContentLength != 0 {
		if err := ler(r, &pedido); err != nil {
			responderErro(w, err)
			return
		}
	}
	s.baixar(w, r, s.whisper, cmpOr(pedido.Ligar, ligarDitado))
}

func (s *Servidor) baixarFala(w http.ResponseWriter, r *http.Request) {
	if !voz.KokoroDisponivel() {
		responderErro(w, dados.ErrInvalido{Motivo: "A voz da abelha ainda não está disponível neste sistema."})
		return
	}
	s.baixar(w, r, s.kokoro, ligarAbelha)
}

// baixar junta o perfil a quem espera o pacote e começa (ou segue) o
// download; pronto e conferido, liga em cada perfil o que ele pediu.
func (s *Servidor) baixar(w http.ResponseWriter, r *http.Request, p *pacoteVoz, ligar string) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if !contem(p.ligas, ligar) {
		responderErro(w, dados.ErrInvalido{Motivo: "Não sei o que ligar com esse modelo."})
		return
	}
	if _, err := s.Banco.Perfil(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	p.quem.juntar(id, ligar)
	err = p.g.Baixar(func(err error) {
		perfis := p.quem.lista(true)
		if err != nil {
			return
		}
		for perfil, ligar := range perfis {
			if err := s.ligarVoz(context.Background(), perfil, ligar); err != nil {
				log.Printf("voz: ligar %s no perfil %d: %v", ligar, perfil, err)
			}
		}
	})
	// Já baixando: este perfil entrou na lista e liga junto.
	if err != nil && err != voz.ErrJaBaixando {
		responderErro(w, err)
		return
	}
	responderJSON(w, p.g.Estado())
}

func contem(lista []string, s string) bool {
	for _, x := range lista {
		if x == s {
			return true
		}
	}
	return false
}

// ligarVoz liga um item no perfil (o modelo acabou de ficar pronto).
func (s *Servidor) ligarVoz(ctx context.Context, perfil int64, o string) error {
	if o == ligarDitado {
		return s.Banco.DefinirDitado(ctx, perfil, true)
	}
	p, err := s.Banco.Perfil(ctx, perfil)
	if err != nil {
		return err
	}
	v := p.OpcoesVoz
	switch o {
	case ligarComandos:
		v.Comandos = true
	case ligarAbelha:
		v.Abelha = true
	}
	return s.Banco.DefinirVoz(ctx, perfil, v)
}

func (s *Servidor) cancelarVoz(w http.ResponseWriter, _ *http.Request) {
	responderJSON(w, map[string]any{"cancelado": s.Voz.Cancelar()})
}

func (s *Servidor) cancelarFala(w http.ResponseWriter, _ *http.Request) {
	responderJSON(w, map[string]any{"cancelado": s.Fala.Cancelar()})
}

// removerVoz apaga o Whisper e desliga, em todos os perfis, o que precisa
// dele: o ditado, os comandos falados e o "ouvir o nome".
func (s *Servidor) removerVoz(w http.ResponseWriter, r *http.Request) {
	if !s.remover(w, s.Voz) {
		return
	}
	perfis, err := s.Banco.PerfisComDitado(r.Context())
	if err != nil {
		responderErro(w, err)
		return
	}
	for _, perfil := range perfis {
		if err := s.Banco.DefinirDitado(r.Context(), perfil, false); err != nil {
			responderErro(w, err)
			return
		}
	}
	if err := s.desligarEmTodos(r.Context(), func(v *dados.OpcoesVoz) { v.Comandos, v.OuvirNome = false, false }); err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, s.Voz.Estado())
}

// removerFala apaga a voz da abelha e a desliga em todos os perfis.
func (s *Servidor) removerFala(w http.ResponseWriter, r *http.Request) {
	if !s.remover(w, s.Fala) {
		return
	}
	if err := s.desligarEmTodos(r.Context(), func(v *dados.OpcoesVoz) { v.Abelha = false }); err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, s.Fala.Estado())
}

func (s *Servidor) remover(w http.ResponseWriter, g *voz.Gerente) bool {
	if err := g.Remover(); err != nil {
		if err == voz.ErrJaBaixando {
			err = dados.ErrInvalido{Motivo: "Espere o download terminar (ou cancele) para apagar o modelo."}
		}
		responderErro(w, err)
		return false
	}
	return true
}

// desligarEmTodos aplica a mudança nos perfis em que ela muda algo.
func (s *Servidor) desligarEmTodos(ctx context.Context, mudar func(*dados.OpcoesVoz)) error {
	perfis, err := s.Banco.ListarPerfis(ctx)
	if err != nil {
		return err
	}
	for _, p := range perfis {
		v := p.OpcoesVoz
		mudar(&v)
		if v == p.OpcoesVoz {
			continue
		}
		if err := s.Banco.DefinirVoz(ctx, p.ID, v); err != nil {
			return err
		}
	}
	return nil
}

// definirDitado é a parte do PATCH do perfil: ligar exige o modelo no disco.
func (s *Servidor) definirDitado(ctx context.Context, perfil int64, ligado bool) error {
	if ligado && !s.Voz.Pronto() {
		return dados.ErrInvalido{Motivo: "Baixe o modelo de voz antes de ligar o ditado."}
	}
	return s.Banco.DefinirDitado(ctx, perfil, ligado)
}

// mudancaVoz são os campos de voz do PATCH do perfil (só os enviados mudam).
type mudancaVoz struct {
	Comandos      *bool `json:"voz_comandos"`
	Abelha        *bool `json:"voz_abelha"`
	Efeito        *bool `json:"voz_efeito"`
	OuvirNome     *bool `json:"voz_nome"`
	Sensibilidade *int  `json:"voz_sensibilidade"`
	PerguntaIA    *bool `json:"voz_ia"`
}

func (m mudancaVoz) vazia() bool {
	return m.Comandos == nil && m.Abelha == nil && m.Efeito == nil && m.OuvirNome == nil && m.Sensibilidade == nil && m.PerguntaIA == nil
}

// definirVoz confere o que cada item precisa antes de ligar: os comandos, o
// Whisper; a voz da abelha, o Kokoro; ouvir o nome, os comandos ligados, a
// abelha fixa (é ela que fica ouvindo, com o microfone sempre à vista) e o
// nome com reconhecimento (o padrão ou as gravações). Desligar os comandos
// desliga o ouvir o nome.
func (s *Servidor) definirVoz(ctx context.Context, perfil int64, m mudancaVoz) error {
	p, err := s.Banco.Perfil(ctx, perfil)
	if err != nil {
		return err
	}
	antes, v := p.OpcoesVoz, p.OpcoesVoz
	aplicar := func(campo **bool, destino *bool) {
		if *campo != nil {
			*destino = **campo
		}
	}
	aplicar(&m.Comandos, &v.Comandos)
	aplicar(&m.Abelha, &v.Abelha)
	aplicar(&m.Efeito, &v.Efeito)
	aplicar(&m.OuvirNome, &v.OuvirNome)
	aplicar(&m.PerguntaIA, &v.PerguntaIA)
	if m.Sensibilidade != nil {
		v.Sensibilidade = *m.Sensibilidade
	}
	if !v.Comandos {
		v.OuvirNome = false
	}
	if v.Comandos && !antes.Comandos && !s.Voz.Pronto() {
		return dados.ErrInvalido{Motivo: "Baixe o modelo de voz antes de ligar os comandos falados."}
	}
	if v.Abelha && !antes.Abelha && (!voz.KokoroDisponivel() || !s.Fala.Pronto()) {
		return dados.ErrInvalido{Motivo: "Baixe a voz da abelha antes de ligar."}
	}
	if v.PerguntaIA && !antes.PerguntaIA {
		if _, err := exec.LookPath(claudecode.Executavel); err != nil {
			return dados.ErrInvalido{Motivo: "A pergunta livre usa o Claude Code, que não está instalado."}
		}
	}
	if v.OuvirNome && !antes.OuvirNome {
		if !p.AbelhaFixa {
			return dados.ErrInvalido{Motivo: "Ouvir o nome precisa da abelha fixa na área de trabalho."}
		}
		if n := s.lerNomeOuvido(p); !n.Padrao && !n.Gravado {
			return dados.ErrInvalido{Motivo: "Grave o nome " + p.MascoteNome + " antes de ligar."}
		}
	}
	return s.Banco.DefinirVoz(ctx, perfil, v)
}

// O nome da abelha que a abelha fixa reconhece. "Colmeia" vem pronto (as
// referências foram feitas com a própria voz sintética e vão dentro do
// colmeia-voz); outro nome precisa de 3 ou 4 gravações, que o colmeia-voz
// transforma em referências (não em áudio) e grava no arquivo do perfil.

// NomePadrao é o nome que já vem com reconhecimento.
const NomePadrao = "colmeia"

// Um arquivo de referências maior que isto não é nosso.
const limiteReferencias = 2 << 20

type nomeOuvido struct {
	Nome    string `json:"nome"`
	Padrao  bool   `json:"padrao"`
	Arquivo string `json:"arquivo"`
	Gravado bool   `json:"gravado"`
}

func (s *Servidor) arquivoDoNome(perfil int64) string {
	return filepath.Join(s.Voz.Pasta(), "nome-"+strconv.FormatInt(perfil, 10)+".json")
}

// lerNomeOuvido: gravado é o arquivo do perfil feito para o nome de agora.
func (s *Servidor) lerNomeOuvido(p dados.Perfil) nomeOuvido {
	n := nomeOuvido{Nome: p.MascoteNome, Padrao: chaveDoNome(p.MascoteNome) == NomePadrao, Arquivo: s.arquivoDoNome(p.ID)}
	f, err := os.Open(n.Arquivo)
	if err != nil {
		return n
	}
	defer f.Close()
	var cabecalho struct {
		Nome string `json:"nome"`
	}
	if json.NewDecoder(io.LimitReader(f, limiteReferencias)).Decode(&cabecalho) == nil {
		n.Gravado = cabecalho.Nome != "" && chaveDoNome(cabecalho.Nome) == chaveDoNome(p.MascoteNome)
	}
	return n
}

// chaveDoNome compara nomes como eles soam: sem acento, caixa e espaços.
func chaveDoNome(s string) string {
	var b strings.Builder
	for _, r := range norm.NFD.String(strings.ToLower(s)) {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			b.WriteRune(r)
		}
	}
	return b.String()
}

func (s *Servidor) nomeOuvido(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	p, err := s.Banco.Perfil(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	// A pasta existe (0700) antes de o colmeia-voz gravar nela.
	if err := os.MkdirAll(s.Voz.Pasta(), 0o700); err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, s.lerNomeOuvido(p))
}

// esquecerNome apaga as gravações do perfil; sem o nome padrão, o ouvir o
// nome desliga junto.
func (s *Servidor) esquecerNome(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	p, err := s.Banco.Perfil(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	if err := os.Remove(s.arquivoDoNome(id)); err != nil && !errors.Is(err, os.ErrNotExist) {
		responderErro(w, err)
		return
	}
	n := s.lerNomeOuvido(p)
	if p.OuvirNome && !n.Padrao {
		v := p.OpcoesVoz
		v.OuvirNome = false
		if err := s.Banco.DefinirVoz(r.Context(), id, v); err != nil {
			responderErro(w, err)
			return
		}
	}
	responderJSON(w, n)
}
