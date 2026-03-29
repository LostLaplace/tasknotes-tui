---
name: task
path_pattern: "TaskNotes/Tasks/{title}.md"
fields:
  title:
    type: string
    required: true
  status:
    type: string
  priority:
    type: string
  due:
    type: date
  scheduled:
    type: date
  recurrence:
    type: string
  recurrenceAnchor:
    type: string
  completeInstances:
    type: list
  skippedInstances:
    type: list
  dateCreated:
    type: datetime
  dateModified:
    type: datetime
---
