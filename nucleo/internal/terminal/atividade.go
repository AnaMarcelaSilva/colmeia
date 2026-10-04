package terminal

import (
	"sync"
	"sync/atomic"
	"time"
)

// Estados de um agente, como o núcleo os vê pela saída do terminal.
const (
	Trabalhando = "trabalhando"
	Ocioso      = "ocioso"
	Aguardando  = "aguardando"
)

const (
	codTrabalhando int32 = iota
	codOcioso
	codAguardando
)

var nomesEstado = [...]string{Trabalhando, Ocioso, Aguardando}

// Tempos do acompanhamento. Os testes usam tempos curtos.
type Tempos struct {
	// Silencio depois do qual o agente parece ter parado e esperar você.
	Silencio time.Duration
	// Ocioso: um terminal comum quieto por esse tempo está parado.
	Ocioso time.Duration
	// Eco: a saída que chega até esse tempo depois da digitação ou de um
	// redimensionamento é eco ou redesenho, não trabalho.
	Eco time.Duration
}

var TemposPadrao = Tempos{Silencio: 5 * time.Second, Ocioso: 60 * time.Second, Eco: 500 * time.Millisecond}

// atividade acompanha um agente sem varredura periódica: cada leitura do
// terminal só grava o momento (atômico); um único timer por sessão olha o
// silêncio quando dispara, e é armado de novo só se ainda houver o que
// decidir. Um agente parado (ocioso ou aguardando) não custa nada até voltar
// a escrever.
type atividade struct {
	ferramenta string
	tempos     Tempos
	ultimos    func(n int) []byte
	avisar     func(Mudanca)

	estado        atomic.Int32
	ultimaSaida   atomic.Int64 // UnixNano
	ultimaEntrada atomic.Int64
	armado        atomic.Bool
	bel           atomic.Bool
	// segurado: quantos pedidos de aprovação do agente esperam você na tela
	// (uma consulta ao banco). Enquanto houver, o estado fica em aguardando,
	// mesmo com o spinner do agente escrevendo.
	segurado atomic.Int32
	// congelado: a Colmeia mandou o programa fechar. O que ele escreve ao
	// sair (o SIGHUP faz o Claude Code redesenhar) não é trabalho e não pode
	// mover o cartão antes de o fim ser gravado.
	congelado atomic.Bool
	osc       uint8 // máquina do BEL; só a goroutine de leitura mexe

	mu          sync.Mutex
	motivo      string
	desde       time.Time
	inicio      time.Time
	trabalhando time.Duration
	aguardando  time.Duration
	timer       *time.Timer
	parado      bool
}

func novaAtividade(ferramenta string, tempos Tempos, ultimos func(int) []byte, avisar func(Mudanca)) *atividade {
	agora := time.Now()
	a := &atividade{ferramenta: ferramenta, tempos: tempos, ultimos: ultimos, avisar: avisar, desde: agora, inicio: agora}
	a.ultimaSaida.Store(agora.UnixNano())
	// Um programa que não escreve nada ao abrir também é avaliado.
	a.armar(tempos.Silencio)
	return a
}

// entrada marca digitação ou redimensionamento vindos da tela.
func (a *atividade) entrada() { a.ultimaEntrada.Store(time.Now().UnixNano()) }

// emEco diz se a saída de agora ainda é eco ou redesenho da última entrada.
func (a *atividade) emEco() bool {
	return time.Now().UnixNano()-a.ultimaEntrada.Load() < int64(a.tempos.Eco)
}

// saida é chamada a cada leitura do terminal. Não aloca nem trava no caso
// comum (o agente já está trabalhando e o timer está armado).
func (a *atividade) saida(b []byte) {
	if a.congelado.Load() {
		return
	}
	bel := a.varrerBEL(b)
	if a.emEco() {
		return
	}
	agora := time.Now().UnixNano()
	if a.segurado.Load() > 0 {
		a.ultimaSaida.Store(agora)
		return
	}
	if bel {
		a.bel.Store(true)
	}
	a.ultimaSaida.Store(agora)
	if a.estado.Load() != codTrabalhando {
		a.mu.Lock()
		a.mudar(codTrabalhando, "")
		a.mu.Unlock()
	}
	if !a.armado.Load() {
		a.armar(a.tempos.Silencio)
	}
}

// varrerBEL acha um BEL (0x07) fora de uma sequência OSC, onde ele só
// termina o título da janela. Fora dela, é o programa pedindo atenção.
func (a *atividade) varrerBEL(b []byte) bool {
	achou := false
	for _, c := range b {
		switch a.osc {
		case 0: // normal
			if c == 0x1b {
				a.osc = 1
			} else if c == 0x07 {
				achou = true
			}
		case 1: // depois de ESC
			switch c {
			case ']':
				a.osc = 2
			case 0x1b:
			default:
				a.osc = 0
			}
		case 2: // dentro de OSC
			if c == 0x07 {
				a.osc = 0
			} else if c == 0x1b {
				a.osc = 3
			}
		case 3: // ESC dentro de OSC
			switch c {
			case '\\':
				a.osc = 0
			case 0x1b:
			default:
				a.osc = 2
			}
		}
	}
	return achou
}

func (a *atividade) armar(d time.Duration) {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.armarComTrava(d)
}

func (a *atividade) armarComTrava(d time.Duration) {
	if a.parado || a.congelado.Load() {
		return
	}
	a.armado.Store(true)
	if a.timer == nil {
		a.timer = time.AfterFunc(d, a.disparou)
	} else {
		a.timer.Reset(d)
	}
}

// disparou decide o estado depois de um tempo sem saída.
func (a *atividade) disparou() {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.armado.Store(false)
	if a.parado || a.estado.Load() != codTrabalhando || a.segurado.Load() > 0 {
		return
	}
	quieto := time.Duration(time.Now().UnixNano() - a.ultimaSaida.Load())
	if quieto < a.tempos.Silencio {
		a.armarComTrava(a.tempos.Silencio - quieto)
		return
	}
	motivo := detectar(a.ferramenta, a.ultimos(4096))
	// As ferramentas de IA param de escrever quando terminam a vez delas (o
	// spinner escreve sem parar enquanto pensam): silêncio é a sua vez. No
	// terminal comum, só se o programa pediu atenção com um BEL.
	if motivo == "" && (a.ferramenta != "shell" || a.bel.Load()) {
		motivo = EsperandoResposta
	}
	if motivo != "" {
		a.bel.Store(false)
		a.mudar(codAguardando, motivo)
		return
	}
	if quieto < a.tempos.Ocioso {
		a.armarComTrava(a.tempos.Ocioso - quieto)
		return
	}
	a.mudar(codOcioso, "")
}

// Motivo do estado aguardando enquanto um pedido do agente espera a sua
// aprovação na tela.
const AprovarConsulta = "aprovar consulta"

// segurar põe o agente em aguardando (motivo fixo) até soltar. Pedidos
// seguidos se somam: o estado só volta quando o último for resolvido.
func (a *atividade) segurar(motivo string) {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.segurado.Add(1)
	a.mudar(codAguardando, motivo)
}

// soltar devolve o agente a trabalhando (ele recebe a resposta e segue) e
// volta a acompanhar o silêncio.
func (a *atividade) soltar() {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.segurado.Add(-1) > 0 {
		return
	}
	a.segurado.Store(0)
	a.ultimaSaida.Store(time.Now().UnixNano())
	a.mudar(codTrabalhando, "")
	a.armarComTrava(a.tempos.Silencio)
}

// congelar mantém o estado atual até o fim: chamado antes de a Colmeia
// fechar o terminal.
func (a *atividade) congelar() {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.congelado.Store(true)
	if a.timer != nil {
		a.timer.Stop()
	}
}

// mudar troca o estado e avisa. Chamar com mu travado.
func (a *atividade) mudar(novo int32, motivo string) {
	if a.parado || a.congelado.Load() {
		return
	}
	atual := a.estado.Load()
	if atual == novo && a.motivo == motivo {
		return
	}
	agora := time.Now()
	a.somar(atual, agora)
	a.estado.Store(novo)
	a.motivo, a.desde = motivo, agora
	a.avisar(Mudanca{Tipo: "estado", Estado: nomesEstado[novo], Motivo: motivo, Desde: agora})
}

func (a *atividade) somar(estado int32, agora time.Time) {
	switch estado {
	case codTrabalhando:
		a.trabalhando += agora.Sub(a.desde)
	case codAguardando:
		a.aguardando += agora.Sub(a.desde)
	}
}

func (a *atividade) atual() (string, string, time.Time) {
	a.mu.Lock()
	defer a.mu.Unlock()
	return nomesEstado[a.estado.Load()], a.motivo, a.desde
}

// parar encerra o acompanhamento e devolve os tempos da sessão.
func (a *atividade) parar() Mudanca {
	a.mu.Lock()
	defer a.mu.Unlock()
	agora := time.Now()
	if !a.parado {
		a.somar(a.estado.Load(), agora)
		a.parado = true
		if a.timer != nil {
			a.timer.Stop()
		}
	}
	return Mudanca{Desde: agora, Duracao: agora.Sub(a.inicio), Trabalhando: a.trabalhando, Aguardando: a.aguardando}
}
