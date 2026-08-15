# Lifecycle De Settlement

Settlement consume un batch seleccionado y produce una transición terminal,
movimientos de activos, accounting de operador, receipt y eventos. Todas las
decisiones se expresan mediante resultados tipados.

## Ruta Principal

```mermaid
sequenceDiagram
    participant C as Caller
    participant B as BatchBook
    participant A as AuctionBook
    participant S as SettlementEngine
    participant R as RiskEngine
    participant L as AccountBook
    participant O as OperatorBook
    C->>B: batch seleccionado
    C->>A: ticket seleccionado
    C->>S: settle_selected
    S->>R: preflight de floor y liquidez
    S->>L: source hacia vault
    S->>L: target hacia recipient y fee
    S->>O: garantía y exposición
    S->>A: mark_settled
    S->>B: close_settled
    S-->>C: receipt
```

## Precondiciones

- batch existente y no terminal;
- bid seleccionado para el mismo batch;
- ticket vigente;
- source y target de la ruta iguales a los de la orden;
- `net_out >= min_out`;
- balance target del vault suficiente para `gross_out`;
- cuentas de payer, recipient, vault y fee registradas.

## Movimientos

```mermaid
flowchart LR
    PS["Payer / source"] -->|amount_in| VS["Vault / source"]
    VT["Vault / target"] -->|net_out| RT["Recipient / target"]
    VT -->|operator_fee| OF["Operator fee / target"]
    OP["Operator pledge"] -->|attachment| RX["Route exposure"]
```

El receipt guarda importes atómicos, IDs de batch/bid/route, operador, assets,
required/attached guarantee y si la ruta fue fallback.

## Fallback

```mermaid
flowchart TD
    W["Selected bid"] --> E{"Settlement result"}
    E -->|ok| S["Settled"]
    E -->|error sin fallback| F["Failed"]
    E -->|error con fallback| N["select_next"]
    N --> A{"Alternativa elegible"}
    A -->|no| F
    A -->|sí| X["Settle alternative"]
    X -->|ok| Z["Selected superseded · Alternative settled"]
    X -->|error| F
```

La alternativa se ordena con el mismo score. El evento
`settlement_fallback` conserva ambos bids y el motivo que impidió completar la
primera ruta.

## Idempotencia Y Terminalidad

Los books impiden reabrir un batch terminal. Una integración debe usar el
estado del batch y el `SettlementKey` como clave de idempotencia. Reenviar un
escenario completo no equivale a reintentar una transición sobre el mismo
runtime: crea un estado determinista nuevo.

## Receipt Operativo

Campos principales:

```json
{
  "operator": "op-alpha",
  "amount_in": 10000,
  "gross_out": 10100,
  "net_out": 10080,
  "operator_fee": 20,
  "required_guarantee": 1515,
  "attached_guarantee": 1515,
  "fallback": false
}
```

La aceptación operativa debe correlacionar receipt, eventos y balances del
snapshot final.

## Recuperacion

Si un batch termina `failed`, conserva el motivo. La recuperación consiste en
crear una nueva orden o corregir capacidad/configuración; nunca en editar el
estado terminal. Los movimientos aplicados deben reconciliarse antes de
reintentar a nivel de negocio.
