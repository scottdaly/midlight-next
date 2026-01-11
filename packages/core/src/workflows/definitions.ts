// Built-in workflow definitions

import type { WorkflowDefinition } from './types.js';

/**
 * Weight Loss Journey Workflow
 */
export const weightLossWorkflow: WorkflowDefinition = {
  id: 'weight-loss',
  name: 'Weight Loss Journey',
  description: 'Track your weight loss goals, meal plans, and progress',
  icon: '⚖️',
  category: 'health',
  projectNameTemplate: 'Weight Loss - {{goal_weight}} Goal',
  projectColor: '#10b981',
  interview: [
    {
      id: 'current_weight',
      question: 'What is your current weight?',
      type: 'text',
      required: true,
      placeholder: 'e.g., 185 lbs or 84 kg',
      helpText: 'Include units (lbs or kg)',
    },
    {
      id: 'goal_weight',
      question: 'What is your target weight?',
      type: 'text',
      required: true,
      placeholder: 'e.g., 165 lbs or 75 kg',
    },
    {
      id: 'timeline',
      question: "What's your target timeline?",
      type: 'select',
      options: ['1 month', '3 months', '6 months', '1 year', 'No specific timeline'],
      required: true,
    },
    {
      id: 'approach',
      question: 'What approaches interest you?',
      type: 'multiselect',
      options: [
        'Diet changes',
        'Exercise/workout routine',
        'Intermittent fasting',
        'Counting calories',
        'Meal prepping',
        'Reducing specific foods',
      ],
      required: false,
      helpText: 'Select all that apply',
    },
    {
      id: 'challenges',
      question: 'What are your biggest challenges?',
      type: 'textarea',
      required: false,
      placeholder: 'e.g., Late night snacking, eating out frequently, lack of time for exercise...',
    },
    {
      id: 'motivation',
      question: 'What motivates you to lose weight?',
      type: 'textarea',
      required: false,
      placeholder: 'e.g., Health reasons, upcoming event, feeling better...',
    },
  ],
  templates: [
    {
      path: 'meal-plan.midlight',
      name: 'Weekly Meal Plan',
      type: 'file',
      generateWithLLM: true,
      llmPrompt: `Create a one-week meal plan for someone who:
- Currently weighs {{current_weight}} and wants to reach {{goal_weight}}
- Has a timeline of {{timeline}}
- Is interested in: {{approach}}
- Faces challenges like: {{challenges}}

Format as a markdown document with:
1. A brief introduction
2. General nutrition guidelines
3. Daily meal suggestions (Monday-Sunday) with breakfast, lunch, dinner, and snacks
4. A shopping list for the week

Keep it practical and achievable.`,
      openAfterCreate: true,
    },
    {
      path: 'workout-plan.midlight',
      name: 'Workout Plan',
      type: 'file',
      generateWithLLM: true,
      llmPrompt: `Create a workout plan for someone who:
- Currently weighs {{current_weight}} and wants to reach {{goal_weight}}
- Has a timeline of {{timeline}}
- Is interested in: {{approach}}

Format as a markdown document with:
1. Weekly schedule overview
2. Detailed daily workouts (include rest days)
3. Tips for beginners
4. How to progress over time

Keep it realistic for someone who may be starting out.`,
    },
    {
      path: 'progress-log.midlight',
      name: 'Progress Log',
      type: 'file',
      contentTemplate: `# Progress Log

## Starting Point
- **Date:** ${new Date().toLocaleDateString()}
- **Starting Weight:** {{current_weight}}
- **Goal Weight:** {{goal_weight}}
- **Timeline:** {{timeline}}

## Weekly Check-ins

### Week 1
| Date | Weight | Notes |
|------|--------|-------|
| | | |

### Week 2
| Date | Weight | Notes |
|------|--------|-------|
| | | |

## Milestones

- [ ] Lost first 5 lbs/2 kg
- [ ] Completed first full week of workouts
- [ ] Stuck to meal plan for a full week
- [ ] Reached halfway point

## Reflections

### What's Working


### What Needs Adjustment

`,
    },
    {
      path: 'recipes/',
      name: 'Recipes',
      type: 'folder',
    },
  ],
  contextSections: {
    overview: 'Weight loss journey from {{current_weight}} to {{goal_weight}} over {{timeline}}. Approaches: {{approach}}.',
    aiNotes: `Help track weight loss progress. Be encouraging but realistic. Suggest healthy alternatives when asked. Focus on sustainable habits over quick fixes. Remember the user's challenges ({{challenges}}) and motivation ({{motivation}}).`,
    initialStatus: 'Just starting the journey. Setting up meal and workout plans.',
    initialDecisions: [
      'Goal: Reach {{goal_weight}} in {{timeline}}',
      'Approaches to try: {{approach}}',
    ],
    initialQuestions: [
      'What supplements might help?',
      'Best time to exercise?',
      'How to handle social eating situations?',
    ],
  },
};

/**
 * Big Purchase Decision Workflow
 */
export const bigPurchaseWorkflow: WorkflowDefinition = {
  id: 'big-purchase',
  name: 'Big Purchase Decision',
  description: 'Research and decide on a major purchase like a car, home, or appliance',
  icon: '🛒',
  category: 'finance',
  projectNameTemplate: '{{item_type}} Purchase Research',
  projectColor: '#6366f1',
  interview: [
    {
      id: 'item_type',
      question: 'What are you looking to buy?',
      type: 'text',
      required: true,
      placeholder: 'e.g., Car, Laptop, House, Refrigerator',
    },
    {
      id: 'budget',
      question: "What's your budget?",
      type: 'text',
      required: true,
      placeholder: 'e.g., $30,000 or $500-1000',
    },
    {
      id: 'timeline',
      question: 'When do you need to make this purchase?',
      type: 'select',
      options: ['Within a week', 'Within a month', '1-3 months', '3-6 months', 'No rush'],
      required: true,
    },
    {
      id: 'priorities',
      question: 'What are your top priorities?',
      type: 'multiselect',
      options: [
        'Price/Value',
        'Quality/Durability',
        'Brand reputation',
        'Features',
        'Aesthetics/Design',
        'Environmental impact',
        'Resale value',
        'Warranty/Support',
      ],
      required: true,
    },
    {
      id: 'must_haves',
      question: 'What features are must-haves?',
      type: 'textarea',
      required: false,
      placeholder: 'List specific features or requirements...',
    },
    {
      id: 'deal_breakers',
      question: "What are your deal breakers?",
      type: 'textarea',
      required: false,
      placeholder: 'Things that would make you not buy something...',
    },
  ],
  templates: [
    {
      path: 'research-notes.midlight',
      name: 'Research Notes',
      type: 'file',
      contentTemplate: `# {{item_type}} Research

## Requirements
- **Budget:** {{budget}}
- **Timeline:** {{timeline}}
- **Priorities:** {{priorities}}

## Must-Haves
{{must_haves}}

## Deal Breakers
{{deal_breakers}}

## Research Notes

### Online Reviews


### Expert Recommendations


### User Experiences

`,
      openAfterCreate: true,
    },
    {
      path: 'comparison.midlight',
      name: 'Options Comparison',
      type: 'file',
      contentTemplate: `# {{item_type}} Comparison

## Options Under Consideration

### Option 1: [Name]
- **Price:**
- **Pros:**
  -
- **Cons:**
  -
- **Rating:** /10

### Option 2: [Name]
- **Price:**
- **Pros:**
  -
- **Cons:**
  -
- **Rating:** /10

### Option 3: [Name]
- **Price:**
- **Pros:**
  -
- **Cons:**
  -
- **Rating:** /10

## Comparison Table

| Feature | Option 1 | Option 2 | Option 3 |
|---------|----------|----------|----------|
| Price | | | |
| | | | |
| | | | |

## Final Ranking
1.
2.
3.
`,
    },
    {
      path: 'decision-log.midlight',
      name: 'Decision Log',
      type: 'file',
      contentTemplate: `# Decision Log

## Final Decision
*To be filled in when decided*

## Decision Date
*Date*

## Chosen Option
*Option name and details*

## Reasoning
*Why this option was chosen*

## Final Price
*Actual price paid*

## Where Purchased
*Store or website*

## Post-Purchase Notes
*How it's working out*
`,
    },
  ],
  contextSections: {
    overview: 'Researching and deciding on a {{item_type}} purchase. Budget: {{budget}}. Timeline: {{timeline}}.',
    aiNotes: `Help research {{item_type}} options within {{budget}}. Priorities are: {{priorities}}. Must-haves: {{must_haves}}. Avoid recommending anything with: {{deal_breakers}}. Be objective and present multiple options with pros and cons.`,
    initialStatus: 'Starting research phase. Gathering options and information.',
    initialDecisions: [
      'Budget set at {{budget}}',
      'Decision needed by {{timeline}}',
    ],
    initialQuestions: [
      'Which brands are most reliable for this category?',
      'Is it better to buy new or used?',
      'Are there upcoming sales or new model releases to wait for?',
    ],
  },
};

/**
 * Book Writing Workflow
 */
export const bookWritingWorkflow: WorkflowDefinition = {
  id: 'book-writing',
  name: 'Book Writing Project',
  description: 'Plan and write a book with structured chapters and notes',
  icon: '📚',
  category: 'creative',
  projectNameTemplate: '{{working_title}}',
  projectColor: '#8b5cf6',
  interview: [
    {
      id: 'working_title',
      question: "What's the working title of your book?",
      type: 'text',
      required: true,
      placeholder: 'e.g., The Last Summer',
    },
    {
      id: 'genre',
      question: 'What genre is your book?',
      type: 'select',
      options: [
        'Fiction - Literary',
        'Fiction - Thriller/Mystery',
        'Fiction - Romance',
        'Fiction - Science Fiction',
        'Fiction - Fantasy',
        'Fiction - Historical',
        'Non-fiction - Memoir',
        'Non-fiction - Self-help',
        'Non-fiction - Business',
        'Non-fiction - History',
        'Non-fiction - Science',
        'Other',
      ],
      required: true,
    },
    {
      id: 'target_length',
      question: 'What is your target word count?',
      type: 'select',
      options: [
        'Short (under 40,000 words)',
        'Novella (40,000-60,000 words)',
        'Standard (60,000-80,000 words)',
        'Long (80,000-100,000 words)',
        'Epic (100,000+ words)',
      ],
      required: true,
    },
    {
      id: 'premise',
      question: 'Describe your book premise in 2-3 sentences',
      type: 'textarea',
      required: true,
      placeholder: 'What is your book about? What makes it unique?',
    },
    {
      id: 'target_audience',
      question: 'Who is your target audience?',
      type: 'textarea',
      required: false,
      placeholder: 'e.g., Young adults interested in fantasy, professionals looking to improve leadership...',
    },
    {
      id: 'writing_schedule',
      question: 'How much time can you dedicate to writing?',
      type: 'select',
      options: [
        'Full-time (30+ hours/week)',
        'Part-time (15-30 hours/week)',
        'Hobby (5-15 hours/week)',
        'Occasional (under 5 hours/week)',
      ],
      required: true,
    },
  ],
  templates: [
    {
      path: 'outline.midlight',
      name: 'Book Outline',
      type: 'file',
      generateWithLLM: true,
      llmPrompt: `Create a book outline for:
Title: "{{working_title}}"
Genre: {{genre}}
Target length: {{target_length}}
Premise: {{premise}}
Audience: {{target_audience}}

Create a structured outline with:
1. A one-paragraph synopsis
2. Chapter-by-chapter breakdown (suggest appropriate number of chapters for the target length)
3. Brief description of each chapter's content
4. Key plot points or themes for each section

Format as a well-organized markdown document.`,
      openAfterCreate: true,
    },
    {
      path: 'characters.midlight',
      name: 'Characters',
      type: 'file',
      contentTemplate: `# Characters

## Main Characters

### [Character Name]
- **Role:** Protagonist/Antagonist/Supporting
- **Age:**
- **Occupation:**
- **Physical Description:**
- **Personality:**
- **Motivation:**
- **Character Arc:**
- **Backstory:**

## Secondary Characters

### [Character Name]
- **Role:**
- **Relationship to main characters:**
- **Key traits:**

## Minor Characters

*List minor characters as they appear*
`,
    },
    {
      path: 'world-building.midlight',
      name: 'World Building',
      type: 'file',
      contentTemplate: `# World Building

## Setting
*Where and when does your story take place?*

## Physical World
- Geography:
- Climate:
- Key locations:

## Society & Culture
- Social structure:
- Customs and traditions:
- Technology level:

## Rules of the World
*Any special rules, magic systems, or unique elements*

## Timeline
*Key historical events relevant to your story*
`,
    },
    {
      path: 'research.midlight',
      name: 'Research Notes',
      type: 'file',
      contentTemplate: `# Research Notes

## Topics to Research

- [ ]
- [ ]
- [ ]

## Research Findings

### [Topic]
*Notes...*

## Sources

*List sources for reference*
`,
    },
    {
      path: 'chapters/',
      name: 'Chapters',
      type: 'folder',
    },
    {
      path: 'drafts/',
      name: 'Drafts',
      type: 'folder',
    },
  ],
  contextSections: {
    overview: '"{{working_title}}" - A {{genre}} book. {{premise}}',
    aiNotes: `Help write "{{working_title}}", a {{genre}} book. Target length: {{target_length}}. Target audience: {{target_audience}}. Writing schedule: {{writing_schedule}}. Assist with plotting, character development, dialogue, prose style, and editing. Maintain consistency with established characters and world-building. Provide constructive feedback while being encouraging.`,
    initialStatus: 'Planning and outlining phase.',
    initialDecisions: [
      'Genre: {{genre}}',
      'Target length: {{target_length}}',
      'Writing schedule: {{writing_schedule}}',
    ],
    initialQuestions: [
      'What is the core theme or message?',
      'What is the narrative voice/POV?',
      "What's the timeline for first draft completion?",
    ],
  },
};

/**
 * All built-in workflows
 */
export const builtInWorkflows: WorkflowDefinition[] = [
  weightLossWorkflow,
  bigPurchaseWorkflow,
  bookWritingWorkflow,
];

/**
 * Get a workflow by ID
 */
export function getWorkflowById(id: string): WorkflowDefinition | undefined {
  return builtInWorkflows.find((w) => w.id === id);
}

/**
 * Get workflows by category
 */
export function getWorkflowsByCategory(category: string): WorkflowDefinition[] {
  return builtInWorkflows.filter((w) => w.category === category);
}
