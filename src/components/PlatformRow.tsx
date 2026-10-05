// Doar tipul folosit de App/LargeView; vechiul rând de platformă a fost înlocuit de LargeView.tsx.
export type ModelScenario = { id: string; modelName: string; requests: number; shortConsumption: number; weeklyConsumption: number; estimatedCost: { perRequestUsd: number; totalUsd: number; inputTokens: number; outputTokens: number } | null; isGeneral: boolean; recommendation?: string };
