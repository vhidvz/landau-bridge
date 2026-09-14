export type SimilarityTone = 'good' | 'fair' | 'weak';

/** The colour band of a similarity percentage; the same cut-offs the review badges used. */
export const similarityTone = (value: number): SimilarityTone => (value >= 80 ? 'good' : value >= 60 ? 'fair' : 'weak');
